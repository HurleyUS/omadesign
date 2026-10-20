//! Gray/Lab document tags: one Undo step, RGB documents only.
use super::*;
use crate::deep_color::tagged::ColorTag;

impl Studio {
    /// Set or clear the document's Gray/Lab tag. Args: tag. Returns whether it changed.
    pub(crate) fn set_color_tag(&mut self, tag: Option<ColorTag>) -> bool {
        if self.doc.color_tag == tag || self.op.is_some() || self.file_dialog_pending() || self.updates.freezing {
            return false;
        }
        if tag.is_some() && self.doc.color.model != crate::deep_color::cms::Model::Rgb {
            self.status = "Gray and Lab tags apply to RGB documents".into();
            return false;
        }
        self.commit(Cmd::SetColorTag { before: self.doc.color_tag, after: tag });
        self.status = format!("{} document", tag.map_or("RGB", ColorTag::name));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn craft_gray_lab_tag_is_one_undo_saves_version_40_and_grays_exports() {
        let mut s = crate::app::symbols::tests::fixture();
        let before = crate::project::encode(&s.doc).unwrap();
        let color = crate::compositor::render_export(&s.doc, 1).unwrap();
        assert!(s.set_color_tag(Some(ColorTag::Gray)));
        assert!(!s.set_color_tag(Some(ColorTag::Gray)));
        let saved = crate::project::encode(&s.doc).unwrap();
        assert!(saved.contains("\"version\":40") || saved.contains("\"version\": 40"), "{}", &saved[..80]);
        let reopened = crate::project::decode(&saved).unwrap();
        assert_eq!(reopened.color_tag, Some(ColorTag::Gray));
        let gray = crate::compositor::render_export(&s.doc, 1).unwrap();
        assert!(gray.pixels().iter().all(|p| { let c = p.demultiply(); c.red() == c.green() && c.green() == c.blue() }));
        assert_ne!(gray.data(), color.data());
        let png = crate::compositor::export_png(&s.doc, 1).unwrap();
        assert_eq!(image::load_from_memory(&png).unwrap().color(), image::ColorType::La8);
        assert!(s.set_color_tag(Some(ColorTag::Lab)));
        assert_eq!(crate::compositor::render_export(&s.doc, 1).unwrap().data(), color.data());
        s.undo();
        s.undo();
        assert_eq!(crate::project::encode(&s.doc).unwrap(), before);
        s.doc.color.model = crate::deep_color::cms::Model::Cmyk;
        assert!(!s.set_color_tag(Some(ColorTag::Lab)));
    }
}
