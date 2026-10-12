//! Precise band controls beside the draggable curve.
use eframe::egui;
use tronteq_shared::{Band, BandKind, DEFAULT_FREQS, NUM_BANDS};

pub fn show(
    ui: &mut egui::Ui,
    bands: &mut [Band; NUM_BANDS],
    selected: &mut usize,
    preamp: &mut f32,
) -> bool {
    *selected = (*selected).min(NUM_BANDS - 1);
    let mut changed = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for i in 0..NUM_BANDS {
            let response = ui.add_sized(
                [28.0, 26.0],
                egui::Button::new((i + 1).to_string()).selected(*selected == i),
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(
                    egui::WidgetType::Button,
                    true,
                    *selected == i,
                    format!("Band {}", i + 1),
                )
            });
            if response
                .on_hover_text(format!("Band {}: {:.0} Hz", i + 1, bands[i].freq))
                .clicked()
            {
                *selected = i;
            }
        }
    });
    ui.add_space(6.0);
    let band = &mut bands[*selected];
    ui.horizontal(|ui| {
        ui.strong(format!("Band {}", *selected + 1));
        if ui
            .small_button("Reset band")
            .on_hover_text("Reset this band's frequency, gain, Q, and type.")
            .clicked()
        {
            *band = Band::flat(DEFAULT_FREQS[*selected]);
            changed = true;
        }
    });
    egui::Grid::new("numeric-band")
        .num_columns(2)
        .spacing([12.0, 9.0])
        .show(ui, |ui| {
            let label = ui.label("Frequency");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut band.freq)
                        .range(20.0..=20_000.0)
                        .speed(5.0)
                        .suffix(" Hz")
                        .max_decimals(1)
                        .update_while_editing(false),
                )
                .labelled_by(label.id)
                .changed();
            ui.end_row();
            let label = ui.label("Type");
            let before = band.kind;
            egui::ComboBox::from_id_salt("numeric-band-kind")
                .width(150.0)
                .selected_text(BandKind::from_u32(band.kind).short_label())
                .show_ui(ui, |ui| {
                    for kind in 0..8 {
                        ui.selectable_value(
                            &mut band.kind,
                            kind,
                            BandKind::from_u32(kind).short_label(),
                        );
                    }
                })
                .response
                .labelled_by(label.id);
            changed |= before != band.kind;
            ui.end_row();
            let label = ui.label("Gain");
            let gain_applies = band.kind <= BandKind::HighShelf as u32;
            changed |= ui
                .add_enabled(
                    gain_applies,
                    egui::DragValue::new(&mut band.gain)
                        .range(-24.0..=24.0)
                        .speed(0.1)
                        .suffix(" dB")
                        .fixed_decimals(1)
                        .update_while_editing(false),
                )
                .labelled_by(label.id)
                .on_disabled_hover_text("This filter type does not use gain.")
                .changed();
            ui.end_row();
            let label = ui.label("Q");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut band.q)
                        .range(0.1..=10.0)
                        .speed(0.01)
                        .fixed_decimals(2)
                        .update_while_editing(false),
                )
                .labelled_by(label.id)
                .changed();
            ui.end_row();
        });
    ui.label(
        egui::RichText::new("Click a value to type, or drag it to adjust.")
            .small()
            .color(crate::theme::muted()),
    );
    ui.separator();
    changed |= ui
        .add(
            egui::Slider::new(preamp, -24.0..=24.0)
                .text("Preamp")
                .suffix(" dB"),
        )
        .changed();
    ui.horizontal(|ui| {
        if ui
            .button("Flat EQ")
            .on_hover_text("Reset all eight bands. Preamp and dynamics are preserved.")
            .clicked()
        {
            *bands = std::array::from_fn(|i| Band::flat(DEFAULT_FREQS[i]));
            changed = true;
        }
        if ui.button("Preamp 0").clicked() {
            *preamp = 0.0;
            changed = true;
        }
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{kittest::Queryable, Harness};
    #[test]
    fn typed_frequency_commits_a_precise_value() {
        let state = ([Band::flat(500.0); NUM_BANDS], 0, 0.0);
        let mut h = Harness::builder()
            .with_size(egui::vec2(310.0, 460.0))
            .build_ui_state(
                |ui, s| {
                    show(ui, &mut s.0, &mut s.1, &mut s.2);
                },
                state,
            );
        h.get_by_label("Frequency").click();
        h.run();
        h.get_by_label("Frequency").type_text("1234.5");
        h.run();
        assert_eq!(h.state().0[0].freq, 500.0, "Typing must wait for commit");
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(h.state().0[0].freq, 1234.5);
        assert_eq!(h.state().0[1].freq, 500.0);
    }
    #[test]
    fn selection_type_and_reset_touch_only_the_selected_band() {
        let state = (
            [Band {
                gain: 4.0,
                ..Band::flat(500.0)
            }; NUM_BANDS],
            0,
            -3.0,
        );
        let mut h = Harness::builder()
            .with_size(egui::vec2(310.0, 460.0))
            .build_ui_state(
                |ui, s| {
                    show(ui, &mut s.0, &mut s.1, &mut s.2);
                },
                state,
            );
        h.get_by_label("Band 8").click();
        h.run();
        assert_eq!(h.state().1, 7);
        h.get_by_value("peak").click();
        h.run();
        h.get_by_label("notch").click();
        h.run();
        assert_eq!(h.state().0[7].kind, BandKind::Notch as u32);
        h.get_by_label("Reset band").click();
        h.run();
        assert_eq!(h.state().0[7], Band::flat(DEFAULT_FREQS[7]));
        assert_eq!(h.state().0[0].gain, 4.0);
        assert_eq!(h.state().2, -3.0);
        h.get_by_label("Flat EQ").click();
        h.run();
        assert!(h
            .state()
            .0
            .iter()
            .all(|band| band.gain == 0.0 && band.kind == 0));
        assert_eq!(h.state().2, -3.0);
    }
}
