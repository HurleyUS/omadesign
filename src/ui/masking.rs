use super::theme::{fg, fg_weak};
use crate::app::Studio;
use crate::artwork_mask::{MaskMode, MaskSource};
use crate::color::Rgba;
use crate::tools::Tool;
use eframe::egui::{Button, RichText, Ui};

#[derive(Clone, Copy)]
pub enum ArtworkAction {
    Source(MaskSource),
    Mode(MaskMode),
    Release,
}

impl ArtworkAction {
    pub fn run(self, studio: &mut Studio, layer: usize, id: Option<u64>) {
        match self {
            Self::Source(source) => studio.select_mask_source(source),
            Self::Mode(mode) => studio.set_artwork_mask_mode(layer, id, mode),
            Self::Release => studio.remove_artwork_masks(layer, id),
        }
    }
}

pub fn artwork_menu(
    ui: &mut Ui,
    studio: &Studio,
    layer: usize,
    id: Option<u64>,
) -> Option<ArtworkAction> {
    let masks = if let Some(id) = id {
        &studio.doc.find_shape(layer, id)?.artwork_masks
    } else {
        &studio.doc.layers.get(layer)?.artwork_masks
    };
    if masks.is_empty() {
        return None;
    }
    let mut action = None;
    ui.separator();
    ui.label("Live artwork mask");
    for (index, mask) in masks.iter().enumerate() {
        let name = studio
            .doc
            .layers
            .iter()
            .find(|l| l.id == mask.source.layer)
            .map(|l| {
                mask.source
                    .shape
                    .and_then(|id| {
                        l.kind
                            .shapes()?
                            .iter()
                            .find(|s| s.id == id)
                            .map(|s| s.name.as_str())
                    })
                    .unwrap_or(&l.name)
            })
            .unwrap_or("Missing source");
        if ui.button(format!("Edit mask source: {name}")).clicked() {
            action = Some(ArtworkAction::Source(mask.source));
        }
        if index == 0 {
            ui.add_enabled_ui(
                studio.layer_unlocked(layer)
                    && id.is_none_or(|id| {
                        studio.doc.find_shape(layer, id).is_some_and(|s| !s.locked)
                    }),
                |ui| {
                    for (mode, label) in [
                        (MaskMode::Alpha, "Use transparency"),
                        (MaskMode::Luminance, "Use luminance"),
                    ] {
                        if ui.selectable_label(mask.mode == mode, label).clicked() {
                            action = Some(ArtworkAction::Mode(mode));
                        }
                    }
                    if ui.button("Release artwork mask").clicked() {
                        action = Some(ArtworkAction::Release);
                    }
                },
            );
        }
    }
    if action.is_some() {
        ui.close();
    }
    action
}

#[derive(Clone, Copy)]
pub enum Action {
    Reveal,
    Hide,
    Selection,
    Invert,
    Remove,
    Apply,
    Edit(bool),
    Filter(crate::app::mask_filter::MaskFilter),
}

impl Action {
    pub fn run(self, studio: &mut Studio, index: usize) {
        match self {
            Self::Reveal => studio.add_layer_mask(index, true),
            Self::Hide => studio.add_layer_mask(index, false),
            Self::Selection => studio.mask_from_selection(index),
            Self::Invert => studio.invert_layer_mask(index),
            Self::Remove => studio.remove_layer_mask(index),
            Self::Apply => studio.apply_layer_mask(index),
            Self::Edit(enabled) => studio.set_mask_edit(index, enabled),
            Self::Filter(filter) => {
                if let Err(error) = studio.filter_layer_mask(index, &filter.filter()) {
                    studio.status = error;
                }
            }
        }
    }
}

pub fn menu(ui: &mut Ui, studio: &Studio, index: usize) -> Option<Action> {
    let layer = studio.doc.layers.get(index)?;
    let editable = !layer.locked && layer.visible;
    let masked = layer.mask.is_some();
    let selected = studio.pixel_sel.is_some();
    let mut action = None;
    let mut item = |ui: &mut Ui, enabled, label, value| {
        if ui.add_enabled(enabled, Button::new(label)).clicked() {
            action = Some(value);
            ui.close();
        }
    };
    item(
        ui,
        editable,
        if masked {
            "Reset to reveal all"
        } else {
            "Reveal all"
        },
        Action::Reveal,
    );
    item(
        ui,
        editable,
        if masked {
            "Reset to hide all"
        } else {
            "Hide all"
        },
        Action::Hide,
    );
    item(
        ui,
        editable && selected,
        if masked {
            "Replace from selection"
        } else {
            "From selection"
        },
        Action::Selection,
    );
    if masked {
        ui.separator();
        item(ui, editable, "Paint mask", Action::Edit(true));
        item(ui, editable, "Invert mask", Action::Invert);
        ui.add_enabled_ui(editable, |ui| {
            ui.menu_button("Filter mask", |ui| {
                for filter in crate::app::mask_filter::MaskFilter::ALL {
                    item(ui, true, filter.label(), Action::Filter(filter));
                }
            })
        });
        item(
            ui,
            editable && layer.kind.pixels().is_some(),
            "Apply to pixels",
            Action::Apply,
        );
        item(ui, editable, "Remove mask", Action::Remove);
    }
    action
}

pub fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let Some(index) = studio
        .active_layer
        .filter(|&index| index < studio.doc.layers.len())
    else {
        ui.label(
            RichText::new("Choose a layer to paint")
                .small()
                .color(fg_weak()),
        );
        return;
    };
    let layer = &studio.doc.layers[index];
    let editable = !layer.locked && layer.visible;
    let pixels = layer.kind.pixels().is_some();
    let masked = layer.mask.is_some();
    let editing = studio.paint_mask && masked;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Paint on").strong().size(12.0).color(fg()));
        ui.add_enabled_ui(editable, |ui| {
            if ui
                .selectable_label(!editing, if pixels { "Pixels" } else { "Artwork" })
                .clicked()
            {
                action = Some(Action::Edit(false));
            }
            if ui
                .add_enabled(masked, Button::new("Mask").selected(editing))
                .clicked()
            {
                action = Some(Action::Edit(true));
            }
        });
        ui.menu_button("···", |ui| {
            if let Some(chosen) = menu(ui, studio, index) {
                action = Some(chosen);
            }
        })
        .response
        .on_hover_text("Layer mask actions");
    });
    if !masked {
        if ui
            .add_enabled(editable, Button::new("Add layer mask"))
            .clicked()
        {
            action = Some(Action::Reveal);
        }
    } else if editing {
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                let black = studio.brush.color.r < 128;
                if ui
                    .selectable_label(black, "Hide")
                    .on_hover_text("Paint black to hide")
                    .clicked()
                {
                    studio.brush.color = Rgba::BLACK;
                    studio.tool = Tool::Brush;
                }
                if ui
                    .selectable_label(!black, "Reveal")
                    .on_hover_text("Paint white to reveal")
                    .clicked()
                {
                    studio.brush.color = Rgba::WHITE;
                    studio.tool = Tool::Brush;
                }
            });
        });
        ui.label(
            RichText::new("Original pixels stay untouched.")
                .small()
                .color(fg_weak()),
        );
    }
    if !editable {
        ui.label(
            RichText::new(if studio.doc.layers[index].locked {
                "Unlock this layer to paint."
            } else {
                "Show this layer to paint."
            })
            .small()
            .color(fg_weak()),
        );
    }
    if let Some(action) = action {
        action.run(studio, index);
    }
}

pub fn selection_tools(ui: &mut Ui, studio: &mut Studio) {
    if studio.pixel_sel.is_none() {
        return;
    }
    ui.label(RichText::new("Selection").small().strong().color(fg()));
    ui.horizontal(|ui| {
        if ui.small_button("←").clicked() {
            studio.nudge_pixel_sel(-1, 0);
        }
        if ui.small_button("→").clicked() {
            studio.nudge_pixel_sel(1, 0);
        }
        if ui.small_button("↑").clicked() {
            studio.nudge_pixel_sel(0, -1);
        }
        if ui.small_button("↓").clicked() {
            studio.nudge_pixel_sel(0, 1);
        }
    });
    ui.menu_button("Modify selection…", |ui| {
        super::pixel_selection::menu(ui, studio);
    });
    ui.label(
        RichText::new("Arrow keys move it while a selection tool is active.")
            .small()
            .color(fg_weak()),
    );
}

pub fn retouch_hint(ui: &mut Ui, studio: &mut Studio) {
    if !matches!(studio.tool, Tool::Clone | Tool::Heal) {
        return;
    }
    if studio.paint_mask {
        ui.label(
            RichText::new("Choose Pixels to retouch the image.")
                .small()
                .color(fg_weak()),
        );
        return;
    }
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(if studio.clone_source.is_some() {
                "Source ready"
            } else {
                "Alt-click to choose a source"
            })
            .small()
            .color(fg_weak()),
        );
        if studio.clone_source.is_some() && ui.small_button("Reset").clicked() {
            studio.clone_source = None;
        }
    });
    if studio.tool == Tool::Heal {
        ui.label(
            RichText::new("Clean texture, blended into local color.")
                .small()
                .color(fg_weak()),
        );
    }
}
