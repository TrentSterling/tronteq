//! The GL viz stage — real shaders under the canvas. This is the demoscene
//! unlock: a ping-pong pair of feedback rendertextures (the Milkdrop trick:
//! last frame gets re-sampled through a warp, then new audio-reactive content
//! splats on top), with the FFT buckets and waveform uploaded as 1D textures
//! and the VizBus stats as uniforms. Painter layers + the EQ curve composite
//! over the result, so every GL mode cross-pollinates with the analyzers.
//!
//! Renders through egui's PaintCallback on the glow backend (raw GL 3.3 core).
//! State discipline: the previous draw-framebuffer binding is saved/restored
//! around the offscreen passes; egui_glow re-establishes its own pipeline
//! state after each callback.

use std::time::Instant;

use eframe::glow::{self, HasContext};

/// Spectrum texture width (matches the analyzer's bin count upper bound).
pub const SPEC_W: usize = 64;
/// Waveform texture width (decimated).
pub const WAVE_W: usize = 256;

/// Everything a shader frame needs, by value (Copy) so the paint callback can
/// own it without borrowing App.
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub mode: GlMode,
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    pub pulse: f32,
    pub beat_phase: f32,
    pub bright: f32,
    pub rainbow: bool,
    pub accent: [f32; 3],
    pub spec: [f32; SPEC_W],
    pub wave: [f32; WAVE_W],
}

impl Default for Uniforms {
    fn default() -> Self {
        Uniforms {
            mode: GlMode::Warp,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            pulse: 0.0,
            beat_phase: 0.0,
            bright: 0.0,
            rainbow: true,
            accent: [0.0, 0.88, 1.0],
            spec: [0.0; SPEC_W],
            wave: [0.0; WAVE_W],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlMode {
    Warp,
    Flame,
    Smoke,
}

/// Owns the GL resources (ids only — the context itself is passed into every
/// call, so this stays Send and can live in the Arc<Mutex> the paint callback
/// captures). Wrapped in Arc<Mutex<..>> on App; the callback locks per frame.
pub struct GlStage {
    prog: [glow::Program; 3], // warp / flame / smoke (index = GlMode)
    present: glow::Program,
    vao: glow::VertexArray,
    fbo: [glow::Framebuffer; 2],
    tex: [glow::Texture; 2],
    size: (i32, i32),
    spec_tex: glow::Texture,
    wave_tex: glow::Texture,
    ping: usize,
    t0: Instant,
}

const VS_FULL: &str = r#"#version 330 core
const vec2 P[3] = vec2[3](vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));
out vec2 vUv;
void main() {
    vec2 p = P[gl_VertexID];
    vUv = p * 0.5 + 0.5;
    gl_Position = vec4(p, 0., 1.);
}"#;

/// Shared fragment prelude: uniforms + helpers every mode uses.
const FRAG_COMMON: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uPrev;
uniform sampler2D uSpec;
uniform sampler2D uWave;
uniform vec2  uRes;
uniform float uTime;
uniform float uBass;
uniform float uMid;
uniform float uTreb;
uniform float uPulse;
uniform float uPhase;
uniform float uBright;
uniform int   uRainbow;
uniform vec3  uAccent;

float spec(float x)  { return texture(uSpec, vec2(clamp(x, 0., 1.), .5)).r; }
float wav(float x)   { return texture(uWave, vec2(clamp(x, 0., 1.), .5)).r; }

vec3 hsv2rgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(0., 2./3., 1./3.)) * 6. - 3.);
    return c.z * mix(vec3(1.), clamp(p - 1., 0., 1.), c.y);
}
float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}
float vnoise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    f = f * f * (3. - 2. * f);
    return mix(mix(hash(i), hash(i + vec2(1., 0.)), f.x),
               mix(hash(i + vec2(0., 1.)), hash(i + vec2(1., 1.)), f.x), f.y);
}
"#;

/// WARP — the Milkdrop playground. Feedback zoom/rotate/decay + a spectrum
/// ring splat that breathes with the beat.
const FRAG_WARP: &str = r#"
void main() {
    vec2 c = vUv - .5;
    c.x *= uRes.x / uRes.y;
    float ang = .0035 + uBass * .028 + sin(uTime * .13) * .004;
    float zoom = .994 - uPulse * .012;
    float ca = cos(ang), sa = sin(ang);
    vec2 w = mat2(ca, -sa, sa, ca) * c * zoom;
    w.x /= uRes.x / uRes.y;
    vec3 prev = texture(uPrev, w + .5).rgb;
    prev *= .962 + uTreb * .022;                  // decay, brightened by highs
    prev = min(prev, vec3(1.2));

    float r = length(c) * 2.;
    float a = atan(c.y, c.x) / 6.28318 + .5;
    float s = spec(fract(a + uPhase));
    float ring = smoothstep(.045, .0, abs(r - (.38 + s * .5 + uPulse * .06)));
    float w0 = wav(vUv.x) * .5;
    float wv = smoothstep(.02, .0, abs(vUv.y - .5 - w0 * .35));
    vec3 base = (uRainbow == 1) ? hsv2rgb(vec3(a + uTime * .02, .85, 1.)) : uAccent;
    vec3 add = base * (ring * (.55 + uPulse * .9) + wv * .18);
    frag = vec4(max(prev, add), 1.);
}
"#;

/// FLAME — heat field stored in the red channel; feedback samples BELOW so
/// heat rises, cooled by noise, fed at the floor by the FFT buckets.
const FRAG_FLAME: &str = r#"
vec3 firePal(float h) {
    h = clamp(h, 0., 1.);
    vec3 c = vec3(h * 1.6, h * h * 1.25, h * h * h * .9);
    return clamp(c, 0., 1.);
}
void main() {
    vec2 px = 1. / uRes;
    float drift = (vnoise(vUv * 9. + uTime * .9) - .5) * 3.5 * px.x;
    vec2 below = vUv + vec2(drift, -(1.6 + uBass * 2.2) * px.y);
    float heat = texture(uPrev, below).a;                    // heat rides alpha
    heat *= .986 - vnoise(vUv * 13. - uTime * 1.7) * .045;   // noisy cooling
    float src = smoothstep(.05, .0, vUv.y) * spec(vUv.x) * (0.9 + uPulse * 1.6);
    float mid = smoothstep(.12, .0, distance(vUv, vec2(.5, .06))) * uPulse * .8;
    heat = max(heat, clamp(src + mid, 0., 1.));
    vec3 col = (uRainbow == 1)
        ? firePal(heat)
        : mix(vec3(0.), uAccent * (heat * 1.4), smoothstep(.02, .6, heat));
    frag = vec4(col, heat);        // rgb = display, a = the feedback field
}
"#;

/// SMOKE — density in the alpha-ish blue channel, advected by curl noise,
/// injected by lows at the floor and pulses at the center.
const FRAG_SMOKE: &str = r#"
vec2 curl(vec2 p) {
    float e = .01;
    float n1 = vnoise(p + vec2(0., e));
    float n2 = vnoise(p - vec2(0., e));
    float n3 = vnoise(p + vec2(e, 0.));
    float n4 = vnoise(p - vec2(e, 0.));
    return vec2(n1 - n2, n4 - n3) / (2. * e);
}
void main() {
    vec2 flow = curl(vUv * 3.2 + vec2(0., uTime * .06)) * (.0016 + uMid * .0045);
    flow.y -= .0014 + uBass * .0028;                     // buoyancy
    float d = texture(uPrev, vUv + flow).a;              // density rides alpha
    d *= .988;
    float floorSrc = smoothstep(.06, .0, vUv.y) * spec(vUv.x) * (.5 + uBass);
    float burst = smoothstep(.16, .0, distance(vUv, vec2(.5))) * uPulse * .9;
    d = clamp(max(d, floorSrc + burst), 0., 1.);
    float shade = d * (.55 + .45 * vnoise(vUv * 6. + uTime * .12));
    vec3 tint = (uRainbow == 1)
        ? hsv2rgb(vec3(.55 + uBright * .35 + vUv.y * .12, .55, shade))
        : uAccent * shade;
    frag = vec4(tint, d);          // rgb = display, a = the feedback field
}
"#;

/// Presents the current feedback texture into the canvas rect.
const VS_PRESENT: &str = r#"#version 330 core
const vec2 P[3] = vec2[3](vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));
uniform vec4 uRect; // xy = min, zw = max, in NDC
out vec2 vUv;
void main() {
    vec2 p = P[gl_VertexID];
    vUv = p * 0.5 + 0.5;
    vec2 ndc = mix(uRect.xy, uRect.zw, vUv);
    gl_Position = vec4(ndc, 0., 1.);
}"#;

const FRAG_PRESENT: &str = r#"#version 330 core
in vec2 vUv;
out vec4 frag;
uniform sampler2D uTex;
void main() { frag = vec4(texture(uTex, vUv).rgb, 1.); }
"#;

impl GlStage {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let compile = |vs: &str, fs: &str| -> Result<glow::Program, String> {
                let program = gl.create_program()?;
                let stages = [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)];
                let mut shaders = Vec::new();
                for (kind, src) in stages {
                    let sh = gl.create_shader(kind)?;
                    gl.shader_source(sh, src);
                    gl.compile_shader(sh);
                    if !gl.get_shader_compile_status(sh) {
                        return Err(gl.get_shader_info_log(sh));
                    }
                    gl.attach_shader(program, sh);
                    shaders.push(sh);
                }
                gl.link_program(program);
                if !gl.get_program_link_status(program) {
                    return Err(gl.get_program_info_log(program));
                }
                for sh in shaders {
                    gl.detach_shader(program, sh);
                    gl.delete_shader(sh);
                }
                Ok(program)
            };

            let warp = compile(VS_FULL, &format!("{FRAG_COMMON}{FRAG_WARP}"))?;
            let flame = compile(VS_FULL, &format!("{FRAG_COMMON}{FRAG_FLAME}"))?;
            let smoke = compile(VS_FULL, &format!("{FRAG_COMMON}{FRAG_SMOKE}"))?;
            let present = compile(VS_PRESENT, FRAG_PRESENT)?;
            let vao = gl.create_vertex_array()?;

            let mk_tex = || -> Result<glow::Texture, String> {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                Ok(t)
            };

            let spec_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, SPEC_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            let wave_tex = mk_tex()?;
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::R32F as i32, WAVE_W as i32, 1, 0,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );

            let fbo = [gl.create_framebuffer()?, gl.create_framebuffer()?];
            let tex = [mk_tex()?, mk_tex()?];
            gl.bind_texture(glow::TEXTURE_2D, None);

            Ok(GlStage {
                prog: [warp, flame, smoke],
                present,
                vao,
                fbo,
                tex,
                size: (0, 0),
                spec_tex,
                wave_tex,
                ping: 0,
                t0: Instant::now(),
            })
        }
    }

    /// (Re)size the ping-pong feedback textures to the canvas pixel size.
    unsafe fn ensure_size(&mut self, gl: &glow::Context, w: i32, h: i32) { unsafe {
        if (w, h) == self.size || w <= 0 || h <= 0 {
            return;
        }
        for i in 0..2 {
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[i]));
            gl.tex_image_2d(
                glow::TEXTURE_2D, 0, glow::RGBA16F as i32, w, h, 0,
                glow::RGBA, glow::FLOAT, glow::PixelUnpackData::Slice(None),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[i]));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D,
                Some(self.tex[i]), 0,
            );
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        self.size = (w, h);
    }}

    /// One feedback pass + present, inside egui's paint callback. `rect_px` is
    /// the canvas rect in physical pixels; `screen_px` the full surface size.
    pub fn paint(&mut self, gl: &glow::Context, u: &Uniforms, rect_px: [f32; 4], screen_px: [f32; 2]) {
        unsafe {
            let w = (rect_px[2] - rect_px[0]).round() as i32;
            let h = (rect_px[3] - rect_px[1]).round() as i32;
            if w < 8 || h < 8 {
                return;
            }
            // Save the framebuffer egui is rendering into.
            let prev_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            self.ensure_size(gl, w, h);

            // Upload audio textures.
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.spec_tex));
            let spec_bytes: &[u8] = core::slice::from_raw_parts(
                u.spec.as_ptr() as *const u8,
                SPEC_W * 4,
            );
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, SPEC_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(spec_bytes)),
            );
            gl.active_texture(glow::TEXTURE2);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.wave_tex));
            let wave_bytes: &[u8] = core::slice::from_raw_parts(
                u.wave.as_ptr() as *const u8,
                WAVE_W * 4,
            );
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D, 0, 0, 0, WAVE_W as i32, 1,
                glow::RED, glow::FLOAT, glow::PixelUnpackData::Slice(Some(wave_bytes)),
            );

            // Feedback pass into the destination texture.
            let src = self.ping;
            let dst = 1 - self.ping;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[dst]));
            gl.viewport(0, 0, w, h);
            gl.disable(glow::BLEND);
            gl.disable(glow::SCISSOR_TEST);
            let prog = self.prog[u.mode as usize];
            gl.use_program(Some(prog));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[src]));
            let loc = |n: &str| gl.get_uniform_location(prog, n);
            gl.uniform_1_i32(loc("uPrev").as_ref(), 0);
            gl.uniform_1_i32(loc("uSpec").as_ref(), 1);
            gl.uniform_1_i32(loc("uWave").as_ref(), 2);
            gl.uniform_2_f32(loc("uRes").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(loc("uTime").as_ref(), self.t0.elapsed().as_secs_f32());
            gl.uniform_1_f32(loc("uBass").as_ref(), u.bass);
            gl.uniform_1_f32(loc("uMid").as_ref(), u.mid);
            gl.uniform_1_f32(loc("uTreb").as_ref(), u.treble);
            gl.uniform_1_f32(loc("uPulse").as_ref(), u.pulse);
            gl.uniform_1_f32(loc("uPhase").as_ref(), u.beat_phase);
            gl.uniform_1_f32(loc("uBright").as_ref(), u.bright);
            gl.uniform_1_i32(loc("uRainbow").as_ref(), if u.rainbow { 1 } else { 0 });
            gl.uniform_3_f32(loc("uAccent").as_ref(), u.accent[0], u.accent[1], u.accent[2]);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);

            // Present into egui's framebuffer at the canvas rect.
            gl.bind_framebuffer(glow::FRAMEBUFFER, if prev_fbo != 0 {
                Some(glow::NativeFramebuffer(
                    std::num::NonZeroU32::new(prev_fbo as u32).unwrap(),
                ))
            } else {
                None
            });
            gl.viewport(0, 0, screen_px[0] as i32, screen_px[1] as i32);
            gl.use_program(Some(self.present));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[dst]));
            let ploc = |n: &str| gl.get_uniform_location(self.present, n);
            gl.uniform_1_i32(ploc("uTex").as_ref(), 0);
            // Rect in NDC (y flipped: GL origin bottom-left, egui top-left).
            let sx = screen_px[0].max(1.0);
            let sy = screen_px[1].max(1.0);
            let x0 = rect_px[0] / sx * 2.0 - 1.0;
            let x1 = rect_px[2] / sx * 2.0 - 1.0;
            let y0 = 1.0 - rect_px[3] / sy * 2.0;
            let y1 = 1.0 - rect_px[1] / sy * 2.0;
            gl.uniform_4_f32(ploc("uRect").as_ref(), x0, y0, x1, y1);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);

            self.ping = dst;
        }
    }
}
