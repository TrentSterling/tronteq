//! The sound library occupies one toolbar slot regardless of profile count.
use eframe::egui;

#[derive(Debug, PartialEq)]
pub enum Action {
    Load(String),
    SaveAs,
    Overwrite(String),
    StartRename(String),
    Delete(String),
}

pub struct Choice<'a> {
    pub name: &'a str,
    pub active: bool,
    pub modified: bool,
}

fn manage(ui: &mut egui::Ui, name: &str, action: &mut Option<Action>) {
    if ui.button("Overwrite with current").clicked() {
        *action = Some(Action::Overwrite(name.into()));
        ui.close();
    }
    if ui.button("Rename...").clicked() {
        *action = Some(Action::StartRename(name.into()));
        ui.close();
    }
    ui.menu_button("Delete", |ui| {
        if ui.button(format!("Really delete \"{name}\"")).clicked() {
            *action = Some(Action::Delete(name.into()));
            ui.close();
        }
    });
}

pub fn show(ui: &mut egui::Ui, choices: &[Choice<'_>], width: f32) -> Option<Action> {
    let mut action = None;
    let active = choices.iter().find(|p| p.active);
    let label = active
        .map(|p| format!("Sound: {}{}", p.name, if p.modified { " *" } else { "" }))
        .unwrap_or_else(|| "Sound: Custom".into());
    ui.allocate_ui_with_layout(egui::vec2(width, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
    ui.set_max_width(width);
    let combo = egui::ComboBox::from_id_salt("sound-profile-picker")
        .selected_text(label).width(width).truncate().height(340.0)
        .show_ui(ui, |ui| {
            ui.set_max_width(300.0);
            if ui.button("Save sound as...").clicked() { action = Some(Action::SaveAs); ui.close(); }
            if let Some(active) = active {
                ui.menu_button("Manage active profile", |ui| manage(ui, active.name, &mut action));
            }
            ui.separator();
            for p in choices {
                let label = format!("{}{}", p.name, if p.modified { " *" } else { "" });
                let response = ui.add(egui::Button::new(label).selected(p.active).truncate());
                if response.clicked() { action = Some(Action::Load(p.name.into())); ui.close(); }
                response.on_hover_text(p.name).context_menu(|ui| manage(ui, p.name, &mut action));
            }
        });
    combo.response.on_hover_text("Sound profiles (keys 1-9). * means modified since save.\nSelect the active profile again to reload it.");
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    #[test]
    fn picker_aligns_with_adjacent_toolbar_controls() {
        let h = Harness::builder().with_size(egui::vec2(640.0, 300.0)).build_ui(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("TrontEQ").size(23.0));
                show(ui, &[Choice { name: "V-shape", active: true, modified: false }], 180.0);
                ui.checkbox(&mut false, "Bypass");
            });
        });
        let picker = h.get_by_value("Sound: V-shape").rect();
        let bypass = h.get_by_label("Bypass").rect();
        assert!((picker.center().y - bypass.center().y).abs() <= 1.0, "{picker:?} {bypass:?}");
        assert!(picker.bottom() <= 40.0);
    }
    #[test]
    fn compact_picker_selects_and_saves_with_many_long_names() {
        let names: Vec<_> = (0..30)
            .map(|i| format!("Very long sound profile number {i}"))
            .collect();
        let mut h = Harness::builder()
            .with_size(egui::vec2(600.0, 450.0))
            .build_ui_state(
                move |ui, action| {
                    let choices: Vec<_> = names
                        .iter()
                        .enumerate()
                        .map(|(i, name)| Choice {
                            name,
                            active: i == 0,
                            modified: i == 0,
                        })
                        .collect();
                    if let Some(a) = show(ui, &choices, 170.0) {
                        *action = Some(a);
                    }
                },
                None,
            );
        let picker = h.get_by_value("Sound: Very long sound profile number 0 *");
        assert!(picker.rect().width() <= 170.0);
        picker.click();
        h.run();
        h.get_by_label("Very long sound profile number 2").click();
        h.run();
        assert_eq!(
            h.state(),
            &Some(Action::Load("Very long sound profile number 2".into()))
        );
        h.get_by_value("Sound: Very long sound profile number 0 *")
            .click();
        h.run();
        h.get_by_label("Save sound as...").click();
        h.run();
        assert_eq!(h.state(), &Some(Action::SaveAs));
    }
}
