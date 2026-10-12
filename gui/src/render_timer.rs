//! Nonblocking GPU timing. A small query ring skips samples while the GPU is busy.
use eframe::glow::{self, HasContext};

pub struct RenderTimer {
    queries: Vec<(glow::Query, bool)>,
    pub milliseconds: Option<f32>,
}
impl RenderTimer {
    pub fn new(gl: &glow::Context) -> Self {
        let mut queries = Vec::new();
        for _ in 0..3 {
            if let Ok(query) = unsafe { gl.create_query() } {
                queries.push((query, false));
            }
        }
        Self {
            queries,
            milliseconds: None,
        }
    }
    pub fn begin(&mut self, gl: &glow::Context) -> bool {
        unsafe {
            for (query, pending) in &mut self.queries {
                if *pending && gl.get_query_parameter_u32(*query, glow::QUERY_RESULT_AVAILABLE) != 0
                {
                    // A 32-bit nanosecond result covers up to 4.29 seconds per frame.
                    // Saturated readings are discarded instead of reporting a false time.
                    let ns = gl.get_query_parameter_u32(*query, glow::QUERY_RESULT);
                    if ns != u32::MAX {
                        let ms = ns as f32 / 1_000_000.0;
                        self.milliseconds =
                            Some(self.milliseconds.map_or(ms, |old| old * 0.9 + ms * 0.1));
                    }
                    *pending = false;
                }
            }
            if let Some((query, pending)) = self.queries.iter_mut().find(|(_, pending)| !pending) {
                gl.begin_query(glow::TIME_ELAPSED, *query);
                *pending = true;
                true
            } else {
                false
            }
        }
    }
    pub fn end(gl: &glow::Context, started: bool) {
        if started {
            unsafe {
                gl.end_query(glow::TIME_ELAPSED);
            }
        }
    }
}
