//! Native text fields retain focus while a clipboard owner answers asynchronously.
use crate::app::Studio;
use eframe::egui::{self, Event, Id, Key};
const JOB: &str = "native-field-paste";
#[derive(Clone)]
struct Owner {
    document: String,
    revision: u64,
    field: Id,
}
type Pasted = (Owner, String);

/// Deliver an OS paste only to the native field that requested it.
/// Args: context and current editor. Returns nothing; canvas clipboard handling remains separate.
pub(super) fn tick(ctx: &egui::Context, studio: &mut Studio) {
    if let Some(result) = super::jobs::poll::<Pasted>(ctx, JOB) {
        let (owner, text) = match result {
            Ok(value) => value,
            Err(error) => {
                studio.status = error;
                return;
            }
        };
        if owner.document == studio.swap_id
            && owner.revision == studio.canvas_gen
            && ctx.memory(|memory| memory.focused()) == Some(owner.field)
            && ctx.input(|input| input.focused)
            && !studio.file_dialog_pending()
            && !studio.updates.freezing
        {
            ctx.input_mut(|input| input.events.push(Event::Paste(text.replace("\r\n", "\n"))));
        }
    }
    if super::jobs::is_running::<Pasted>(ctx, JOB) {
        return;
    }
    let Some(field) = ctx.memory(|memory| memory.focused()) else {
        return;
    };
    if field == Id::new("studio-canvas") || field == Id::new("studio-photo-canvas") {
        return;
    }
    let requested=ctx.input(|input|input.focused&&!input.events.iter().any(|event|matches!(event,Event::Paste(_)))&&input.events.iter().any(|event|matches!(event,
        Event::Key{key:Key::V,pressed:true,modifiers,..} if (modifiers.command||modifiers.ctrl||modifiers.mac_cmd)&&!modifiers.alt
    )||matches!(event,Event::Key{key:Key::Insert,pressed:true,modifiers,..} if modifiers.shift)));
    if requested && !studio.file_dialog_pending() && !studio.updates.freezing {
        let owner = Owner {
            document: studio.swap_id.clone(),
            revision: studio.canvas_gen,
            field,
        };
        super::jobs::start(ctx, JOB, move || {
            crate::clipboard::type_style::read(None).map(|paste| (owner, paste.text))
        });
    }
}
