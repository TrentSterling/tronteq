//! Custom window chrome: TrontEQ runs `.with_decorations(false)`, so the OS title
//! bar is gone and the toolbar IS the title bar. This module owns the three jobs
//! the OS used to do — the minimize / maximize / close buttons, moving the window,
//! and resizing it from the edges. Ported from Boxel's window_chrome.rs (the
//! campaign pattern; TrontSnap has the same treatment).
//!
//! Why: the toolbar already draws the wordmark + profile chips, so a stock Windows
//! caption above it was a second, mismatched bar wasting ~30px.
//!
//! What we give up: the Win11 hover-maximize snap flyout and system rounded
//! corners (drag-to-edge Aero Snap still works — `StartDrag` hands off to the OS).
//! Judged not worth a Win32 `WM_NCHITTEST` subclass, same call as Boxel.
//!
//! Close routes through `ViewportCommand::Close`, so the existing close-intercept
//! in `update()` still runs — on TrontEQ that means hide-to-tray, not exit.

use crate::theme;
use egui::{Color32, Sense, Stroke, StrokeKind};

/// Which caption button to paint. `Maximize` / `Restore` are the two faces of the
/// same slot, picked from the live viewport state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WinBtn {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Paint one caption button: a hover fill (reddish for close, the theme hover tint
/// otherwise) plus a hand-drawn glyph. Glyphs are painted as line segments rather
/// than text so they never depend on Rajdhani having box / dash / multiply
/// coverage (the `→`-tofu lesson).
pub fn window_button(ui: &mut egui::Ui, kind: WinBtn) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(34.0, 24.0), Sense::click());
    if resp.hovered() {
        let fill = match kind {
            WinBtn::Close => Color32::from_rgb(200, 60, 60),
            _ => ui.visuals().widgets.hovered.bg_fill,
        };
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let glyph = if resp.hovered() && kind == WinBtn::Close {
        Color32::WHITE
    } else {
        theme::text()
    };
    let stroke = Stroke::new(1.3, glyph);
    let painter = ui.painter();
    let c = rect.center();
    match kind {
        WinBtn::Minimize => {
            painter.line_segment([egui::pos2(c.x - 5.0, c.y), egui::pos2(c.x + 5.0, c.y)], stroke);
        }
        WinBtn::Maximize => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                1.0,
                stroke,
                StrokeKind::Middle,
            );
        }
        WinBtn::Restore => {
            // Two offset outlines = the standard "restore down" mark.
            let back = egui::Rect::from_center_size(c + egui::vec2(2.5, -2.5), egui::vec2(8.0, 8.0));
            let front = egui::Rect::from_center_size(c + egui::vec2(-2.0, 2.0), egui::vec2(8.0, 8.0));
            painter.rect_stroke(back, 1.0, stroke, StrokeKind::Middle);
            painter.rect_stroke(front, 1.0, stroke, StrokeKind::Middle);
        }
        WinBtn::Close => {
            painter.line_segment(
                [egui::pos2(c.x - 5.0, c.y - 5.0), egui::pos2(c.x + 5.0, c.y + 5.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 5.0, c.y + 5.0), egui::pos2(c.x + 5.0, c.y - 5.0)],
                stroke,
            );
        }
    }
    resp
}

/// Is the window maximized right now? Drives the Maximize/Restore glyph and the
/// resize-strip skip.
pub fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized).unwrap_or(false)
}

/// Width the caption overlay occupies at the window's top-right. The toolbar
/// reserves this much so its content stops short of the buttons at normal
/// widths; when the window gets TOO narrow the overlay simply covers whatever
/// slid under it (opaque background — nothing draws through).
pub const CAPTION_W: f32 = 3.0 * 34.0 + 10.0;

/// The caption buttons as a floating always-on-top overlay pinned to the
/// window's top-right corner, with an opaque panel-colored background. Being a
/// Foreground `Area` (not part of the toolbar row) means a cramped toolbar can
/// never z-fight the min/max/close targets — they win, always.
pub fn caption_overlay(ctx: &egui::Context) {
    let maximized = is_maximized(ctx);
    egui::Area::new(egui::Id::new("tronteq_caption_overlay"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .corner_radius(egui::CornerRadius { sw: 6, ..Default::default() })
                .inner_margin(egui::Margin { left: 6, right: 4, top: 3, bottom: 3 })
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.horizontal(|ui| {
                        if window_button(ui, WinBtn::Minimize).on_hover_text("Minimize").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                        let max_kind = if maximized { WinBtn::Restore } else { WinBtn::Maximize };
                        if window_button(ui, max_kind).on_hover_text("Maximize / restore").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                        }
                        if window_button(ui, WinBtn::Close)
                            .on_hover_text("Hide to tray (EQ keeps running)")
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                });
        });
}

/// Turn an already-allocated `Response` into a window move handle: drag moves the
/// window, double-click toggles maximize. Used for the toolbar gap and the
/// wordmark itself, so a packed bar always has a grab point.
pub fn drag_window(ctx: &egui::Context, resp: &egui::Response) {
    if resp.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    } else if resp.double_clicked() {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized(ctx)));
    }
}

/// Make an already-laid-out rect a window move handle. Deliberately `interact` on
/// a known rect rather than allocating a filler widget: allocating
/// `available_size()` inside a `right_to_left` block grows the row past the panel
/// and pushes the right-hand controls off-screen (Boxel gotcha).
pub fn drag_region(ui: &mut egui::Ui, rect: egui::Rect, id_salt: &str) {
    if rect.width() < 8.0 {
        return; // bar packed edge to edge; the wordmark is still a grab point
    }
    let ctx = ui.ctx().clone();
    let resp = ui.interact(rect, ui.id().with(id_salt), Sense::click_and_drag());
    drag_window(&ctx, &resp);
}

/// Thin invisible interact strips along the 4 edges + 4 corners, so the window can
/// still be resized by dragging even though `.with_decorations(false)` removed the
/// OS resize border. Each strip is its own foreground `Area`, so it takes priority
/// over panel content underneath. Skipped while maximized/fullscreen.
pub fn edge_resize(ctx: &egui::Context) {
    let (maximized, fullscreen) = ctx.input(|i| {
        (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false))
    });
    if maximized || fullscreen {
        return;
    }
    use egui::viewport::ResizeDirection as Dir;
    use egui::CursorIcon as Cur;

    let screen = ctx.input(|i| i.screen_rect());
    const EDGE: f32 = 6.0;
    const CORNER: f32 = 10.0;

    let strips: [(egui::Rect, Dir, Cur); 8] = [
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.left() + CORNER, screen.top()),
                egui::pos2(screen.right() - CORNER, screen.top() + EDGE),
            ),
            Dir::North,
            Cur::ResizeVertical,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.left() + CORNER, screen.bottom() - EDGE),
                egui::pos2(screen.right() - CORNER, screen.bottom()),
            ),
            Dir::South,
            Cur::ResizeVertical,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.left(), screen.top() + CORNER),
                egui::pos2(screen.left() + EDGE, screen.bottom() - CORNER),
            ),
            Dir::West,
            Cur::ResizeHorizontal,
        ),
        (
            egui::Rect::from_min_max(
                egui::pos2(screen.right() - EDGE, screen.top() + CORNER),
                egui::pos2(screen.right(), screen.bottom() - CORNER),
            ),
            Dir::East,
            Cur::ResizeHorizontal,
        ),
        (
            egui::Rect::from_min_size(screen.left_top(), egui::vec2(CORNER, CORNER)),
            Dir::NorthWest,
            Cur::ResizeNwSe,
        ),
        (
            egui::Rect::from_min_size(
                egui::pos2(screen.right() - CORNER, screen.top()),
                egui::vec2(CORNER, CORNER),
            ),
            Dir::NorthEast,
            Cur::ResizeNeSw,
        ),
        (
            egui::Rect::from_min_size(
                egui::pos2(screen.left(), screen.bottom() - CORNER),
                egui::vec2(CORNER, CORNER),
            ),
            Dir::SouthWest,
            Cur::ResizeNeSw,
        ),
        (
            egui::Rect::from_min_size(
                egui::pos2(screen.right() - CORNER, screen.bottom() - CORNER),
                egui::vec2(CORNER, CORNER),
            ),
            Dir::SouthEast,
            Cur::ResizeNwSe,
        ),
    ];

    for (i, (rect, dir, cursor)) in strips.into_iter().enumerate() {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue; // window smaller than 2*CORNER mid-restore; nothing to hit
        }
        egui::Area::new(egui::Id::new("tronteq_resize_edge").with(i))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .show(ctx, |ui| {
                let (_, resp) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                if resp.hovered() {
                    ui.output_mut(|o| o.cursor_icon = cursor);
                }
                if resp.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                }
            });
    }
}
