//! Startup must show the window without crawling the user's files or waiting on fonts.
use crate::app::Studio;
use std::time::{Duration, Instant};

const CHILD: &str = "OMADESIGN_STARTUP_TEST_CHILD";
/// Studio::new budget in an unoptimized test build; release is far below this.
const STUDIO_NEW_BUDGET: Duration = Duration::from_millis(1500);

/// Run this test again in a fresh process with an isolated HOME so global font
/// state is untouched. Returns false when already running as that child.
fn in_child(name: &str, home: &std::path::Path) -> bool {
    if std::env::var_os(CHILD).is_some() {
        return false;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "startup child failed: {status}");
            return true;
        }
        if started.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            panic!("startup blocked: it read a file under ~/Projects (the trap FIFO never answers)");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn trapped_home(tag: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!("omadesign-startup-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let app = home.join("Projects/site/app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("layout.tsx"), r#"import { Inter } from "next/font/google";"#).unwrap();
    // Opening a FIFO for reading blocks until a writer appears, so any crawl that
    // reads source files under ~/Projects hangs here and the parent fails the test.
    let fifo = app.join("trap.tsx");
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    home
}

#[test]
fn startup_never_walks_projects_and_never_waits_on_fonts() {
    let name = "startup_tests::startup_never_walks_projects_and_never_waits_on_fonts";
    if std::env::var_os(CHILD).is_none() {
        let home = trapped_home("walk");
        assert!(in_child(name, &home));
        let _ = std::fs::remove_dir_all(&home);
        return;
    }
    let started = Instant::now();
    let mut studio = Studio::new();
    let elapsed = started.elapsed();
    assert!(!crate::text::fonts_ready(), "Studio::new enumerated system fonts on the UI thread");
    assert!(studio.font_default_pending);
    assert!(elapsed < STUDIO_NEW_BUDGET, "Studio::new took {elapsed:?}, budget {STUDIO_NEW_BUDGET:?}");
    // Resolving the default font later must not crawl projects either.
    crate::text::start_font_index();
    studio.settle_default_font(true);
    assert!(!studio.font_default_pending);
    assert!(crate::text::fonts_ready());
}

#[test]
fn font_index_is_cached_and_reused_on_the_next_start() {
    let name = "startup_tests::font_index_is_cached_and_reused_on_the_next_start";
    if std::env::var_os(CHILD).is_none() {
        let home = trapped_home("index");
        assert!(in_child(name, &home));
        let index = home.join("data/omadesign/font-index-v1.tsv");
        let first = std::fs::read_to_string(&index).expect("font index written");
        let modified = std::fs::metadata(&index).unwrap().modified().unwrap();
        assert!(first.lines().count() > 1, "index lists installed faces");
        assert!(in_child(name, &home));
        assert_eq!(std::fs::metadata(&index).unwrap().modified().unwrap(), modified, "a valid index is reused, not rewritten");
        let _ = std::fs::remove_dir_all(&home);
        return;
    }
    assert!(!crate::text::fonts().is_empty());
}

#[test]
fn deferred_brush_packs_match_a_synchronous_load() {
    let dir = std::env::temp_dir().join(format!("omadesign-startup-brushes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("broken.omabrush"), b"not a pack").unwrap();
    let sync = crate::brushes::Library::load(dir.clone());
    let mut deferred = crate::brushes::Library::load_deferred(dir.clone());
    let started = Instant::now();
    while deferred.user_packs_pending() {
        deferred.poll_user_packs();
        assert!(started.elapsed() < Duration::from_secs(30), "user packs never arrived");
        std::thread::sleep(Duration::from_millis(5));
    }
    let names = |l: &crate::brushes::Library| l.packs.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&deferred), names(&sync));
    assert_eq!(deferred.errors, sync.errors);
    assert!(!sync.errors.is_empty(), "the unreadable pack is reported");
    let _ = std::fs::remove_dir_all(&dir);
}
