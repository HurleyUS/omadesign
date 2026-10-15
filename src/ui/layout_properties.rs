//! Named properties edit existing native fields and retain ordinary instance overrides.
use crate::app::{ComponentPropertyEdit as Edit, Studio};
use crate::document::Fill;
use crate::geom::Geom;
use crate::layout_components::{
    self as components, ComponentBinding, PropertyKind as Kind, PropertyValue as Value,
};
use eframe::egui::{self, Ui};

fn text(ui: &mut Ui, id: impl std::hash::Hash, name: &str, value: &str) -> Option<String> {
    let key = ui.make_persistent_id(id);
    let mut draft = ui
        .data(|data| data.get_temp::<String>(key))
        .unwrap_or_else(|| value.into());
    let label = ui.label(name);
    let response = ui
        .add(egui::TextEdit::singleline(&mut draft).id(key))
        .labelled_by(label.id);
    let commit = response.lost_focus()
        || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && response.has_focus() {
        draft = value.into();
        response.surrender_focus();
    }
    if commit {
        ui.data_mut(|d| d.remove::<String>(key));
        (draft != value).then_some(draft)
    } else {
        if response.has_focus() {
            ui.data_mut(|d| d.insert_temp(key, draft));
        }
        None
    }
}

pub(super) fn inspector(ui: &mut Ui, studio: &mut Studio) {
    let Some(target) = studio.component_property_target() else {
        return;
    };
    let (layer, root) = (target.layer, target.root);
    let Some((_, selected)) = studio.primary() else {
        return;
    };
    let Some(shape) = studio.doc.find_shape(layer, root) else {
        return;
    };
    let main = matches!(shape.layout.component, Some(ComponentBinding::Main { .. }));
    let properties = components::properties(&studio.doc, root);
    let blocked = studio.file_dialog_pending()
        || studio.pending_nav.is_some()
        || studio.updates.freezing
        || studio.op.is_some();
    let mut edit = None;
    let mut select = None;
    egui::CollapsingHeader::new("Named properties")
        .default_open(!properties.is_empty())
        .show(ui, |ui| {
            ui.add_enabled_ui(!blocked, |ui| {
                ui.push_id((layer, root, selected, "named-properties"), |ui| {
                    for property in properties {
                        ui.push_id(property.id, |ui| {
                            match components::property_value(&studio.doc, layer, root, property.id)
                            {
                                Ok(Value::Text(value)) => {
                                    if let Some(value) = text(ui, "value", &property.name, &value) {
                                        edit = Some(Edit::Set {
                                            id: property.id,
                                            value: Value::Text(value),
                                        });
                                    }
                                }
                                Ok(Value::Visibility(mut value)) => {
                                    if ui.checkbox(&mut value, &property.name).changed() {
                                        edit = Some(Edit::Set {
                                            id: property.id,
                                            value: Value::Visibility(value),
                                        });
                                    }
                                }
                                Ok(Value::Opacity(mut value)) => {
                                    value *= 100.;
                                    let response = super::fields::number(
                                        ui,
                                        "value",
                                        &property.name,
                                        &mut value,
                                        0.0..=100.0,
                                        " %",
                                        None,
                                        Default::default(),
                                    );
                                    if response.changed {
                                        edit = Some(Edit::Set {
                                            id: property.id,
                                            value: Value::Opacity(value / 100.),
                                        });
                                    }
                                }
                                Ok(Value::Fill(Fill::Solid(color))) => {
                                    if let Some(value) = text(
                                        ui,
                                        "value",
                                        &format!("{} · hex", property.name),
                                        &color.hex(),
                                    ) {
                                        if let Some(color) = crate::color::Rgba::parse_hex(&value) {
                                            edit = Some(Edit::Set {
                                                id: property.id,
                                                value: Value::Fill(Fill::Solid(color)),
                                            });
                                        } else {
                                            studio.status =
                                                "Enter a hex color, such as #336699".into();
                                        }
                                    }
                                }
                                Ok(Value::Fill(_)) => {
                                    ui.label(&property.name);
                                    if ui.button("Edit paint on object").clicked() {
                                        select = components::property_object(
                                            &studio.doc,
                                            layer,
                                            root,
                                            property.id,
                                        )
                                        .ok();
                                    }
                                }
                                Ok(Value::Variant(mut current)) => {
                                    let old = current;
                                    let family = components::family_of(&studio.doc, current);
                                    egui::ComboBox::from_id_salt("value")
                                        .selected_text(
                                            components::definitions(&studio.doc)
                                                .iter()
                                                .find(|(_, id, _, _)| *id == current)
                                                .map(|(_, _, _, v)| v.as_str())
                                                .unwrap_or("Missing variant"),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (_, id, _, variant) in
                                                components::definitions(&studio.doc)
                                                    .into_iter()
                                                    .filter(|(_, id, _, _)| {
                                                        components::family_of(&studio.doc, *id)
                                                            == family
                                                    })
                                            {
                                                ui.selectable_value(&mut current, id, variant);
                                            }
                                        })
                                        .response
                                        .on_hover_text(&property.name);
                                    ui.label(&property.name);
                                    if current != old {
                                        edit = Some(Edit::Set {
                                            id: property.id,
                                            value: Value::Variant(current),
                                        });
                                    }
                                }
                                Err(error) => {
                                    ui.label(&property.name);
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                            }
                            if main {
                                if let Some(name) =
                                    text(ui, "name", "Property name", &property.name)
                                {
                                    edit = Some(Edit::Rename {
                                        id: property.id,
                                        name,
                                    });
                                }
                                if ui
                                    .small_button(format!("Remove {}", property.name))
                                    .clicked()
                                {
                                    edit = Some(Edit::Remove { id: property.id });
                                }
                            }
                        });
                    }
                    if main {
                        ui.separator();
                        ui.label("Expose selected object field");
                        let key = ui.make_persistent_id("new-name");
                        let mut name = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_default();
                        let label = ui.label("New property name");
                        ui.add(egui::TextEdit::singleline(&mut name).id(key))
                            .labelled_by(label.id);
                        ui.data_mut(|d| d.insert_temp(key, name.clone()));
                        if let Some(shape) = studio.doc.find_shape(layer, selected) {
                            let mut kinds = vec![Kind::Visibility, Kind::Fill, Kind::Opacity];
                            if matches!(shape.geom, Geom::Text(_)) {
                                kinds.insert(0, Kind::Text);
                            }
                            if matches!(
                                shape.layout.component,
                                Some(ComponentBinding::Instance { .. })
                            ) {
                                kinds.push(Kind::Variant);
                            }
                            for kind in kinds {
                                if ui
                                    .add_enabled(
                                        !name.trim().is_empty(),
                                        egui::Button::new(format!("Expose {}", kind.name())),
                                    )
                                    .clicked()
                                {
                                    edit = Some(Edit::Expose {
                                        object: selected,
                                        name: name.clone(),
                                        kind,
                                    });
                                }
                            }
                        }
                    }
                });
            });
        });
    if let Some(edit) = edit {
        studio.apply_component_property(target, edit);
    }
    if let Some(id) = select {
        studio.selection = vec![(layer, id)];
    }
}
