use crate::viz_looks::Look;
use crate::{
    show::ShowMode,
    viz_motion::{self, Runtime as Motion, Source},
    viz_performance::{self as perf, Cue, Runtime},
    viz_settings::{Effect, EffectSlot, VizSettings},
};
use eframe::egui;

pub fn transforms(ui: &mut egui::Ui, c: &mut VizSettings, live: &Motion) {
    egui::CollapsingHeader::new("Frame movement").show(ui, |ui| {
        for (i, (key, label)) in [
            ("zoom", "Zoom"),
            ("rotation", "Rotation"),
            ("pan_x", "Horizontal"),
            ("pan_y", "Vertical"),
        ]
        .into_iter()
        .enumerate()
        {
            let (lo, hi) = viz_motion::bounds(key).unwrap();
            ui.add(egui::Slider::new(&mut c.transform[i], lo..=hi).text(label));
            viz_motion::binding(ui, key, c.transform[i], &mut c.motion, live);
        }
        if ui.small_button("Reset movement").clicked() {
            c.transform = [1.0, 0.0, 0.0, 0.0];
        }
    });
}
pub fn flash(ui: &mut egui::Ui, c: &mut VizSettings, live: &Motion) {
    egui::CollapsingHeader::new("Flash control").show(ui, |ui| {
        let mut enabled = c
            .effects
            .iter()
            .any(|s| s.effect == Effect::Strobe && s.enabled);
        if ui.checkbox(&mut enabled, "Beat flash").changed() {
            if let Some(slot) = c.effects.iter_mut().find(|s| s.effect == Effect::Strobe) {
                slot.enabled = enabled;
            } else if enabled {
                c.effects.push(EffectSlot {
                    effect: Effect::Strobe,
                    enabled: true,
                    mix: 1.0,
                });
            }
        }
        ui.small("Controls the Beat flash effect. Scene animation keeps its own lighting.");
        egui::ComboBox::from_id_salt("flash-source")
            .selected_text(c.flash.source.label())
            .show_ui(ui, |ui| {
                for source in Source::ALL {
                    ui.selectable_value(&mut c.flash.source, source, source.label());
                }
            });
        ui.add(egui::ProgressBar::new(live.signals[c.flash.source as usize]).desired_height(5.0));
        ui.add(egui::Slider::new(&mut c.flash.amount, 0.0..=1.0).text("Strength"));
        viz_motion::binding(ui, "flash_amount", c.flash.amount, &mut c.motion, live);
        ui.add(egui::Slider::new(&mut c.flash.threshold, 0.0..=1.0).text("Trigger threshold"));
        ui.add(
            egui::Slider::new(&mut c.flash.decay, 0.03..=2.0)
                .text("Decay")
                .suffix(" s"),
        );
        ui.add(
            egui::Slider::new(&mut c.flash.max_rate, 0.25..=12.0)
                .text("Maximum rate")
                .suffix(" /s"),
        );
        if ui.button("Disable Beat flash").clicked() {
            c.flash.source = Source::Off;
            for s in &mut c.effects {
                if s.effect == Effect::Strobe {
                    s.enabled = false;
                }
            }
        }
    });
}
pub fn signals(ui: &mut egui::Ui, c: &mut VizSettings, live: &mut Motion) {
    ui.strong("Signal lab");
    signal_response(ui, &mut c.signal_response, live);
    ui.small("Original input is the thin marker; the fill is the signal your visuals receive.");
    for source in Source::ALL.into_iter().skip(1) {
        ui.push_id(source as usize, |ui| {
            if ui.selectable_label(live.conditioning.selected == source, source.label()).clicked() { live.conditioning.selected=source; }
            let (r,_) = ui.allocate_exact_size(egui::vec2(ui.available_width(),7.0),egui::Sense::hover());
            let p=ui.painter();p.rect_filled(r,2,ui.visuals().extreme_bg_color);
            p.rect_filled(egui::Rect::from_min_size(r.min,egui::vec2(r.width()*live.signals[source as usize],r.height())),2,crate::theme::cyan());
            let x=r.left()+r.width()*live.conditioning.raw[source as usize];
            p.line_segment([egui::pos2(x,r.top()-1.0),egui::pos2(x,r.bottom()+1.0)],egui::Stroke::new(1.0,ui.visuals().text_color()));
        });
    }
    ui.small("Stereo balance: left 0%, center 50%, right 100%. Tonal/noisy is an FFT texture estimate, not instrument recognition. Kick onset covers roughly 60–190 Hz.");
    flash(ui, c, live);
}

pub fn signal_response(ui: &mut egui::Ui, c: &mut crate::viz_signals::Settings, live: &mut Motion) {
    use crate::viz_signals::{Settings, adaptive};
    ui.checkbox(&mut c.enabled,"Adaptive response").on_hover_text("Learns the music's range and shapes movement. Saved with this look; audio processing is unchanged.");
    ui.horizontal_wrapped(|ui| {
        for (name,a,r) in [("Musical",0.025,0.18),("Percussive",0.0,0.09),("Flowing",0.2,0.65)] {
            if ui.small_button(name).clicked() { *c=Settings::preset(a,r);live.conditioning.reset(); }
        }
        if ui.small_button("Original").clicked() { c.enabled=false; }
    });
    if !c.enabled { return; }
    let source = if live.conditioning.selected==Source::Off { Source::Bass } else { live.conditioning.selected };
    live.conditioning.selected=source;
    egui::ComboBox::from_id_salt("shape-signal").selected_text(source.label()).show_ui(ui,|ui| {
        for s in Source::ALL.into_iter().skip(1) { ui.selectable_value(&mut live.conditioning.selected,s,s.label()); }
    });
    let source=live.conditioning.selected;
    let channel=&mut c.channels[source as usize];
    if adaptive(source) { ui.add(egui::Slider::new(&mut channel.adapt,0.0..=1.0).text("Adapt range")); }
    else { ui.small("This signal keeps its absolute meaning. Shape its timing below."); }
    ui.add(egui::Slider::new(&mut channel.attack,0.0..=2.0).text("Attack").suffix(" s"));
    ui.add(egui::Slider::new(&mut channel.release,0.0..=3.0).text("Release").suffix(" s"));
    let i=source as usize;
    ui.small(format!("Input {:.0}% | Output {:.0}%",live.conditioning.raw[i]*100.0,live.signals[i]*100.0));
    egui::CollapsingHeader::new("Learning and quiet input").show(ui,|ui| {
        ui.add(egui::Slider::new(&mut c.memory,1.0..=30.0).text("Memory").suffix(" s"));
        ui.add(egui::Slider::new(&mut c.floor_db,-80.0..=-25.0).text("Quiet floor").suffix(" dBFS"));
        let range=live.conditioning.learned[i];
        if adaptive(source) { ui.small(format!("Learned range {:.0}%–{:.0}%",range[0]*100.0,range[1]*100.0)); }
        ui.small(if live.conditioning.active { "Learning from audio" } else { "Below quiet floor; energy signals release toward zero" });
        if ui.small_button("Relearn now").clicked() { live.conditioning.reset(); }
    });
}

pub fn interaction_recipe(index: usize, mode: &mut ShowMode, c: &mut VizSettings) {
    use crate::viz_compositor::{Interaction,InteractionMode};
    let (a,b,kind,amount) = match index {
        0 => (ShowMode::GlTerrain,ShowMode::GlFlame,InteractionMode::Flow,0.6),
        1 => (ShowMode::GlKaleido,ShowMode::Spectrogram,InteractionMode::Displace,0.55),
        _ => (ShowMode::GlCopper,ShowMode::GlFlame,InteractionMode::Light,0.8),
    };
    let mut look=Look { name: "Interaction A".into(),scene:a.as_str().into(),speed:0.65,signal_response:crate::viz_signals::Settings::preset(0.025,0.18),..Default::default() };
    look.motion.routes.insert("deck_distortion".into(),viz_motion::Route {source:Source::Beat,depth:0.2,..Default::default()});
    look.apply(mode,c);
    c.performance.interaction=Interaction {mode:kind,amount,..Default::default()};
    c.performance.deck_b=Some(Look {name:b.label().into(),scene:b.as_str().into(),speed:0.65,..Default::default()});
    c.performance.crossfade=0.35;
    c.performance.enabled=true;
}

fn interaction(ui: &mut egui::Ui, mode: &mut ShowMode, c: &mut VizSettings, live: &Motion) -> bool {
    use crate::viz_compositor::{InteractionMode,Direction};
    let mut recipe_changed=false;
    egui::CollapsingHeader::new("Cross-deck distortion").default_open(c.performance.interaction.mode!=InteractionMode::Off).show(ui,|ui| {
        ui.small("Use one scene as a field that bends or lights the other. Strongest between the crossfader ends.");
        ui.horizontal_wrapped(|ui| {for (i,name) in ["Flame flow","Spectral glass","Prism light"].iter().enumerate() {
            if ui.small_button(*name).clicked() {interaction_recipe(i,mode,c);recipe_changed=true;}
        }});
        let d=&mut c.performance.interaction;
        egui::ComboBox::from_id_salt("deck-interaction").selected_text(d.mode.label()).show_ui(ui,|ui| {
            for mode in InteractionMode::ALL {ui.selectable_value(&mut d.mode,mode,mode.label());}
        });
        if d.mode==InteractionMode::Off {return;}
        egui::ComboBox::from_id_salt("deck-direction").selected_text(d.direction.label()).show_ui(ui,|ui| {
            for direction in Direction::ALL {ui.selectable_value(&mut d.direction,direction,direction.label());}
        });
        if !c.performance.enabled || c.performance.deck_b.is_none() {ui.small("Load and enable deck B below to see the interaction.");}
        for (key,label,v,lo,hi) in [
            ("deck_distortion","Strength",&mut d.amount,0.0,1.0),
            ("deck_scale","Field scale",&mut d.scale,0.25,4.0),
            ("deck_angle","Angle",&mut d.angle,-std::f32::consts::PI,std::f32::consts::PI),
            ("deck_softness","Softness",&mut d.softness,0.0,1.0),
        ] {
            ui.add(egui::Slider::new(v,lo..=hi).text(label));
            viz_motion::binding(ui,key,*v,&mut c.motion,live);
        }
    });
    recipe_changed
}
pub fn racks(ui: &mut egui::Ui, c: &mut VizSettings, rt: &mut Runtime) {
    egui::CollapsingHeader::new("Effect racks")
        .default_open(true)
        .show(ui, |ui| {
            let mut pick = None;
            egui::ComboBox::from_id_salt("rack-library")
                .selected_text("Load a rack...")
                .show_ui(ui, |ui| {
                    for rack in perf::factory_racks().iter().chain(c.saved_racks.iter()) {
                        if ui.selectable_label(false, &rack.name).clicked() {
                            pick = Some(rack.clone());
                        }
                    }
                });
            if let Some(rack) = pick {
                rack.apply(c);
                rt.notice = format!("Loaded {}", rack.name);
            }
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut rt.rack_name)
                        .hint_text("Rack name")
                        .desired_width(125.0)
                        .char_limit(80),
                );
                let exists = c.saved_racks.iter().any(|r| r.name == rt.rack_name.trim());
                if ui
                    .add_enabled(
                        !rt.rack_name.trim().is_empty(),
                        egui::Button::new(if exists { "Replace" } else { "Save rack" }),
                    )
                    .clicked()
                {
                    let rack = perf::Rack::capture(rt.rack_name.trim().into(), c);
                    if let Some(r) = c.saved_racks.iter_mut().find(|r| r.name == rack.name) {
                        *r = rack;
                    } else {
                        c.saved_racks.push(rack);
                    }
                    rt.notice = "Effect rack saved".into();
                }
                ui.menu_button("Manage", |ui| {
                    let mut remove = None;
                    for (i, rack) in c.saved_racks.iter().enumerate() {
                        if ui.button(format!("Delete {}", rack.name)).clicked() {
                            remove = Some(i);
                        }
                    }
                    if let Some(i) = remove {
                        c.saved_racks.remove(i);
                        ui.close();
                    }
                });
            });
            ui.small("Racks replace effects and their routes; the scene and palette stay yours.");
        });
}
pub fn show(
    ui: &mut egui::Ui,
    mode: &mut ShowMode,
    c: &mut VizSettings,
    rt: &mut Runtime,
    live: &mut Motion,
    shuffle: &mut bool,
) {
    ui.strong("Performance pads");
    ui.small("Hold a pad or keys 1–5 while this page is open. Release to return to your look.");
    ui.horizontal_wrapped(|ui| {
        for (i, label) in perf::PADS.into_iter().enumerate() {
            let response = ui.add_sized(
                [92.0, 34.0],
                egui::Button::new(format!("{} {label}", i + 1)).selected(rt.pads[i] > 0.1),
            );
            rt.held[i] = response.is_pointer_button_down_on();
        }
    });
    ui.add(egui::Slider::new(&mut c.performance.pad_strength, 0.0..=1.0).text("Pad strength"));
    ui.add(
        egui::Slider::new(&mut c.performance.release, 0.03..=2.0)
            .text("Return")
            .suffix(" s"),
    );
    egui::CollapsingHeader::new("Drive pads with music").show(ui, |ui| {
        for (i, label) in perf::PADS.into_iter().enumerate() {
            ui.label(label);
            viz_motion::binding(ui, &format!("pad:{i}"), 0.0, &mut c.motion, live);
        }
    });
    ui.separator();
    ui.strong("Two-deck mixer");
    egui::ComboBox::from_id_salt("deck-blend").selected_text(c.performance.blend.label()).show_ui(ui,|ui|{for blend in crate::viz_compositor::Blend::ALL {ui.selectable_value(&mut c.performance.blend,blend,blend.label());}});
    if c.performance.blend!=crate::viz_compositor::Blend::Crossfade{ui.small("Blend character peaks at the midpoint; each end remains a clean deck.");}
    ui.label(format!("A: {}", mode.label()));
    ui.checkbox(&mut c.performance.enabled, "Enable deck B");
    let mut pick = None;
    egui::ComboBox::from_id_salt("deck-b-look")
        .selected_text(
            c.performance
                .deck_b
                .as_ref()
                .map_or("Load B...", |l| l.name.as_str()),
        )
        .show_ui(ui, |ui| {
            for look in crate::viz_looks::factory()
                .iter()
                .chain(c.saved_looks.iter())
                .filter(|l| l.scene != "off")
            {
                if ui.selectable_label(false, &look.name).clicked() {
                    pick = Some(look.clone());
                }
            }
        });
    if let Some(look) = pick {
        c.performance.deck_b = Some(look);
        c.performance.enabled = true;
        rt.motion_b = Default::default();
    }
    if ui.small_button("Copy current look to B").clicked() {
        c.performance.deck_b = Some(Look::capture("Deck B", *mode, c));
        c.performance.enabled = true;
    }
    if ui
        .add(egui::Slider::new(&mut c.performance.crossfade, 0.0..=1.0).text("A ↔ B"))
        .changed()
    {
        rt.fade = None;
        rt.playing = false;
    }
    viz_motion::binding(
        ui,
        "crossfade",
        c.performance.crossfade,
        &mut c.motion,
        live,
    );
    if interaction(ui, mode, c, live) { rt.fade=None;rt.playing=false;rt.motion_b=Default::default(); }
    ui.horizontal_wrapped(|ui| {
        if ui.button("Fade to A").clicked() {
            rt.fade_to(&c.performance, 0.0);
        }
        if ui
            .add_enabled(
                c.performance.deck_b.is_some(),
                egui::Button::new("Fade to B"),
            )
            .clicked()
        {
            c.performance.enabled = true;
            rt.fade_to(&c.performance, 1.0);
        }
        if ui.small_button("Take B to A").clicked() {
            if let Some(look) = c.performance.deck_b.clone() {
                let old = Look::capture("Previous A", *mode, c);
                look.apply(mode, c);
                c.performance.deck_b = Some(old);
                c.performance.crossfade = 1.0 - c.performance.crossfade;
                c.performance.blend_reverse=!c.performance.blend_reverse;
                c.performance.interaction.direction=c.performance.interaction.direction.swapped();
                rt.fade = None;
                rt.playing = false;
                rt.swap_decks = true;
            }
        }
    });
    ui.add(
        egui::Slider::new(&mut c.performance.fade_beats, 0.0..=32.0)
            .step_by(1.0)
            .text("Fade beats"),
    );
    ui.checkbox(&mut c.performance.follow_bpm, "Follow detected tempo");
    ui.add(
        egui::Slider::new(&mut c.performance.manual_bpm, 40.0..=240.0)
            .text("Fallback / manual BPM"),
    );
    ui.small("Low-confidence tempo uses the manual clock. Fades advance in beats.");
    let rack_choices: Vec<_> = perf::factory_racks()
        .into_iter()
        .chain(c.saved_racks.iter().cloned())
        .collect();
    if let Some(look) = &mut c.performance.deck_b {
        egui::CollapsingHeader::new("Edit deck B").show(ui, |ui| {
            ui.push_id("deck-b-controls", |ui| {
                egui::ComboBox::from_id_salt("b-scene")
                    .selected_text(ShowMode::from_str(&look.scene).label())
                    .show_ui(ui, |ui| {
                        for scene in ShowMode::ALL.into_iter().skip(1) {
                            ui.selectable_value(
                                &mut look.scene,
                                scene.as_str().into(),
                                scene.label(),
                            );
                        }
                    });
                ui.add(egui::Slider::new(&mut look.speed, 0.0..=2.0).text("Speed"));
                ui.add(egui::Slider::new(&mut look.energy, 0.0..=2.0).text("Audio response"));
                ui.add(egui::Slider::new(&mut look.brightness, 0.1..=1.5).text("Brightness"));
                ui.add(egui::Slider::new(&mut look.hue, 0.0..=1.0).text("Color shift"));
                for (key, base) in [
                    ("speed", look.speed),
                    ("energy", look.energy),
                    ("brightness", look.brightness),
                    ("hue", look.hue),
                ] {
                    viz_motion::binding(ui, key, base, &mut look.motion, &rt.motion_b);
                }
                egui::ComboBox::from_id_salt("b-rack")
                    .selected_text("Rack for B...")
                    .show_ui(ui, |ui| {
                        for rack in &rack_choices {
                            if ui.selectable_label(false, &rack.name).clicked() {
                                let mut temp = VizSettings::default();
                                let mut m = ShowMode::Off;
                                look.apply(&mut m, &mut temp);
                                rack.apply(&mut temp);
                                *look = Look::capture(&look.name, m, &temp);
                            }
                        }
                    });
                for slot in &mut look.effects {
                    ui.checkbox(&mut slot.enabled, slot.effect.label());
                    ui.add(egui::Slider::new(&mut slot.mix, 0.0..=1.0).text("Mix"));
                    viz_motion::binding(
                        ui,
                        &viz_motion::effect_key(slot.effect.bit()),
                        slot.mix,
                        &mut look.motion,
                        &rt.motion_b,
                    );
                }
                let mut temp = VizSettings::default();
                let mut m = ShowMode::Off;
                look.apply(&mut m, &mut temp);
                if let Some(labels) = crate::viz_panel::scene_control_labels(m) {
                    let values = temp
                        .scene_controls
                        .entry(m.as_str().into())
                        .or_insert([1.0; 4]);
                    for (i, label) in labels.into_iter().enumerate() {
                        ui.add(egui::Slider::new(&mut values[i], 0.0..=2.0).text(label));
                        viz_motion::binding(
                            ui,
                            &viz_motion::scene_key(m.as_str(), i),
                            values[i],
                            &mut temp.motion,
                            &rt.motion_b,
                        );
                    }
                }
                transforms(ui, &mut temp, &rt.motion_b);
                flash(ui, &mut temp, &rt.motion_b);
                egui::CollapsingHeader::new("Deck B signal response").show(ui,|ui| {
                    signal_response(ui,&mut temp.signal_response,&mut rt.motion_b);
                });
                *look = Look::capture(&look.name, m, &temp);
            });
        });
    }
    ui.separator();
    egui::CollapsingHeader::new("Mutate this look")
        .default_open(true)
        .show(ui, |ui| {
            if rt.mutation == 0.0 && rt.rng == 0 {
                rt.mutation = 0.25;
                rt.locks = [true, true, false, false];
                rt.rng = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
            }
            ui.add(egui::Slider::new(&mut rt.mutation, 0.0..=1.0).text("Mutation amount"));
            ui.horizontal_wrapped(|ui| {
                for (i, label) in ["Lock scene", "Lock palette", "Lock motion", "Lock effects"]
                    .into_iter()
                    .enumerate()
                {
                    ui.checkbox(&mut rt.locks[i], label);
                }
            });
            if ui.button("Roll mutation").clicked() {
                perf::mutate(mode, c, rt.mutation, rt.locks, &mut rt.rng);
                rt.notice = "Mutation applied. Visual Undo restores it.".into();
            }
        });
    transforms(ui, c, live);
    flash(ui, c, live);
    egui::CollapsingHeader::new("Visual setlist")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Add current look").clicked() {
                    let look = Look::capture(
                        &format!("{} {}", mode.label(), c.performance.setlist.len() + 1),
                        *mode,
                        c,
                    );
                    if *mode != ShowMode::Off {
                        c.performance.setlist.push(Cue {
                            look,
                            hold_beats: 32.0,
                            fade_beats: 8.0,
                        });
                    }
                }
                if ui.button("Start setlist").clicked() {
                    *shuffle = false;
                    rt.start(mode, c);
                }
                if ui.button("Stop").clicked() {
                    rt.playing = false;
                    rt.fade = None;
                }
                if ui.button("Next cue").clicked() {
                    *shuffle = false;
                    rt.next(c);
                }
            });
            ui.checkbox(&mut c.performance.loop_setlist, "Loop setlist");
            if rt.playing {
                ui.label(format!("Cue {} · {:.1} beats", rt.cue + 1, rt.held_beats));
            }
            let mut remove = None;
            let mut up = None;
            for (i, cue) in c.performance.setlist.iter_mut().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("{}.", i + 1));
                        ui.add(egui::TextEdit::singleline(&mut cue.look.name).desired_width(120.0));
                        if ui.add_enabled(i > 0, egui::Button::new("↑")).clicked() {
                            up = Some(i);
                        }
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.add(
                        egui::Slider::new(&mut cue.hold_beats, 1.0..=256.0)
                            .step_by(1.0)
                            .text("Hold beats"),
                    );
                    ui.add(
                        egui::Slider::new(&mut cue.fade_beats, 0.0..=64.0)
                            .step_by(1.0)
                            .text("Fade beats"),
                    );
                });
            }
            if let Some(i) = remove {
                c.performance.setlist.remove(i);
                rt.playing = false;
                rt.fade = None;
            }
            if let Some(i) = up {
                c.performance.setlist.swap(i, i - 1);
                rt.playing = false;
                rt.fade = None;
            }
        });
    if !rt.notice.is_empty() {
        ui.small(&rt.notice);
    }
}
