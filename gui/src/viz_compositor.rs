//! Deck blending and pulse shaping happen after scene effects, before UI overlays.
use eframe::{
    egui,
    glow::{self, HasContext},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Blend {
    #[default]
    Crossfade,
    Add,
    Screen,
    Difference,
    Frequency,
}
impl Blend {
    pub const ALL: [Self; 5] = [
        Self::Crossfade,
        Self::Add,
        Self::Screen,
        Self::Difference,
        Self::Frequency,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Crossfade => "Crossfade",
            Self::Add => "Additive",
            Self::Screen => "Screen",
            Self::Difference => "Difference",
            Self::Frequency => "Frequency mask",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionMode { #[default] Off, Displace, Flow, Light }
impl InteractionMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::Displace, Self::Flow, Self::Light];
    pub fn label(self) -> &'static str {
        match self { Self::Off => "Off", Self::Displace => "Displace", Self::Flow => "Flow", Self::Light => "Light" }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction { #[default] BtoA, AtoB, Both }
impl Direction {
    pub const ALL: [Self; 3] = [Self::BtoA, Self::AtoB, Self::Both];
    pub fn label(self) -> &'static str {
        match self { Self::BtoA => "B shapes A", Self::AtoB => "A shapes B", Self::Both => "Both ways" }
    }
    pub fn swapped(self) -> Self {
        match self { Self::BtoA => Self::AtoB, Self::AtoB => Self::BtoA, Self::Both => Self::Both }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Interaction {
    pub mode: InteractionMode,
    pub direction: Direction,
    pub amount: f32,
    pub scale: f32,
    pub angle: f32,
    pub softness: f32,
}
impl Default for Interaction {
    fn default() -> Self { Self { mode: InteractionMode::Off, direction: Direction::BtoA, amount: 0.45, scale: 1.0, angle: 0.0, softness: 0.25 } }
}
impl Interaction {
    pub fn sanitize(&mut self) {
        let f = |v: f32, lo: f32, hi: f32, fallback: f32| if v.is_finite() { v.clamp(lo,hi) } else { fallback };
        self.amount=f(self.amount,0.0,1.0,0.45);
        self.scale=f(self.scale,0.25,4.0,1.0);
        self.angle=f(self.angle,-std::f32::consts::PI,std::f32::consts::PI,0.0);
        self.softness=f(self.softness,0.0,1.0,0.25);
    }
}
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pulse {
    pub amount: f32,
    pub punch: f32,
    pub ceiling: f32,
    pub recovery: f32,
}
impl Default for Pulse {
    fn default() -> Self {
        Self {
            amount: 0.0,
            punch: 0.4,
            ceiling: 0.85,
            recovery: 0.2,
        }
    }
}
impl Pulse {
    pub fn sanitize(&mut self) {
        fn finite(v: f32, lo: f32, hi: f32, default: f32) -> f32 {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                default
            }
        }
        self.amount = finite(self.amount, 0.0, 1.0, 0.0);
        self.punch = finite(self.punch, 0.0, 1.0, 0.4);
        self.ceiling = finite(self.ceiling, 0.1, 1.5, 0.85);
        self.recovery = finite(self.recovery, 0.02, 2.0, 0.2);
    }
    pub fn array(self) -> [f32; 4] {
        [self.amount, self.punch, self.ceiling, self.recovery]
    }
}
pub fn panel(ui: &mut egui::Ui, p: &mut Pulse) {
    egui::CollapsingHeader::new("Global pulse control").show(ui,|ui|{ui.small("Shapes brightness changes in the final artwork, including both decks and scene lighting.");ui.add(egui::Slider::new(&mut p.amount,0.0..=1.0).text("Control amount"));ui.add(egui::Slider::new(&mut p.punch,0.0..=1.0).text("Punch"));ui.add(egui::Slider::new(&mut p.ceiling,0.1..=1.5).text("Brightness ceiling"));ui.add(egui::Slider::new(&mut p.recovery,0.02..=2.0).text("Recovery").suffix(" s"));ui.small("0% preserves the original image. Lower punch softens sudden rises; recovery smooths falls.");});
}

const VS: &str = r#"#version 330 core
out vec2 uv;const vec2 P[3]=vec2[3](vec2(-1),vec2(3,-1),vec2(-1,3));void main(){gl_Position=vec4(P[gl_VertexID],0,1);uv=P[gl_VertexID]*.5+.5;}
"#;
const FS: &str = r#"#version 330 core
in vec2 uv;out vec4 frag;uniform sampler2D uA,uB,uPrev,uSpec,uOtherSpec;uniform int uPass,uBlend,uReverse,uFirst;uniform float uMix,uDt;uniform vec4 uPulse;
uniform int uInteraction,uDirection;uniform vec4 uInteractionControls;
vec2 mirrored(vec2 p){return 1.-abs(mod(p,2.)-1.);}
float luminance(vec3 c){return dot(c,vec3(.2126,.7152,.0722));}
vec3 shaped(sampler2D artwork,sampler2D field,float strength){
    vec2 p=mirrored((uv-.5)*uInteractionControls.y+.5);
    vec3 m=texture(field,p).rgb;
    vec2 texel=1./vec2(textureSize(field,0));
    vec2 stepSize=texel*mix(1.,12.,uInteractionControls.w);
    float gx=luminance(texture(field,mirrored(p+vec2(stepSize.x,0))).rgb)-luminance(texture(field,mirrored(p-vec2(stepSize.x,0))).rgb);
    float gy=luminance(texture(field,mirrored(p+vec2(0,stepSize.y))).rgb)-luminance(texture(field,mirrored(p-vec2(0,stepSize.y))).rgb);
    float angle=uInteractionControls.z;mat2 rotation=mat2(cos(angle),sin(angle),-sin(angle),cos(angle));
    vec2 offset=vec2(m.r-m.b,m.g-.5*(m.r+m.b));
    if(uInteraction==2)offset=clamp(vec2(gy,-gx)*4.,vec2(-1),vec2(1));
    if(uInteraction==3){
        vec3 a=texture(artwork,uv).rgb;
        float relief=dot(vec2(gx,gy),vec2(cos(angle),sin(angle)))*2.;
        vec3 light=vec3(.3)+m*1.25+vec3(relief);
        return mix(a,a*max(light,vec3(0)),strength);
    }
    return texture(artwork,mirrored(uv+rotation*offset*strength*.18)).rgb;
}
void main(){vec3 a=texture(uA,uv).rgb;vec3 c=a;
if(uPass==0){vec3 b=texture(uB,uv).rgb;float t=clamp(uMix,0.,1.);
float middle=4.*t*(1.-t);
if(uInteraction>0&&uInteractionControls.x>0.&&middle>0.){
    float strength=uInteractionControls.x*middle;
    if(uDirection==0||uDirection==2)a=shaped(uA,uB,strength);
    if(uDirection==1||uDirection==2)b=shaped(uB,uA,strength);
}
c=mix(a,b,t);
if(uBlend==1)c=mix(c,min(a+b,vec3(1)),middle);
if(uBlend==2)c=mix(c,1.-(1.-a)*(1.-b),middle);
if(uBlend==3)c=mix(c,abs(a-b),middle);
if(uBlend==4){float energy=.5*(texture(uSpec,vec2(uv.x,.5)).r+texture(uOtherSpec,vec2(uv.x,.5)).r);float field=.6*uv.y+.4*(1.-energy);if(uReverse!=0)field=1.-field;float m=t<=0.?0.:t>=1.?1.:smoothstep(field-.08,field+.08,t);c=mix(a,b,m);}}
if(uPass==1){vec3 prior=texture(uPrev,uv).rgb;float l=dot(a,vec3(.2126,.7152,.0722));float peak=max(a.r,max(a.g,a.b));float ceiling=max(.1,uPulse.z);float knee=ceiling*.7;float mapped=peak<=knee?peak:knee+(ceiling-knee)*(1.-exp(-(peak-knee)/(ceiling-knee)));vec3 target=a*(mapped/max(peak,.00001));float old=dot(prior,vec3(.2126,.7152,.0722));float tau=l>old?mix(.35,.005,uPulse.y):max(.02,uPulse.w);float rate=1.-exp(-max(uDt,0.)/tau);vec3 limited=uFirst!=0?target:mix(prior,target,rate);c=mix(a,limited,uPulse.x);}
frag=vec4(clamp(c,0.,1.),1);}
"#;
pub struct Output {
    last: Option<glow::Texture>,
    program: glow::Program,
    vao: glow::VertexArray,
    tex: [glow::Texture; 3],
    fbo: [glow::Framebuffer; 3],
    size: (i32, i32),
    ping: usize,
    primed: bool,
    epoch: u64,
}
impl Output {
    pub fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            for (kind, src) in [(glow::VERTEX_SHADER, VS), (glow::FRAGMENT_SHADER, FS)] {
                let sh = gl.create_shader(kind)?;
                gl.shader_source(sh, src);
                gl.compile_shader(sh);
                if !gl.get_shader_compile_status(sh) {
                    let e = gl.get_shader_info_log(sh);
                    gl.delete_shader(sh);
                    for s in shaders {
                        gl.delete_shader(s);
                    }
                    gl.delete_program(program);
                    return Err(e);
                }
                gl.attach_shader(program, sh);
                shaders.push(sh);
            }
            gl.link_program(program);
            let valid = gl.get_program_link_status(program);
            let error = gl.get_program_info_log(program);
            for s in shaders {
                gl.detach_shader(program, s);
                gl.delete_shader(s);
            }
            if !valid {
                gl.delete_program(program);
                return Err(error);
            }
            Ok(Self {
                last: None,
                program,
                vao: gl.create_vertex_array()?,
                tex: [
                    gl.create_texture()?,
                    gl.create_texture()?,
                    gl.create_texture()?,
                ],
                fbo: [
                    gl.create_framebuffer()?,
                    gl.create_framebuffer()?,
                    gl.create_framebuffer()?,
                ],
                size: (0, 0),
                ping: 0,
                primed: false,
                epoch: 0,
            })
        }
    }
    pub fn reset(&mut self) {
        self.primed = false;
        self.last = None;
    }
    pub fn present(&self, gl: &glow::Context, vp: [i32; 4]) {
        unsafe {
            if let Some(tex) = self.last {
                gl.viewport(vp[0], vp[1], vp[2], vp[3]);
                gl.enable(glow::SCISSOR_TEST);
                gl.disable(glow::BLEND);
                gl.use_program(Some(self.program));
                gl.bind_vertex_array(Some(self.vao));
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                gl.uniform_1_i32(gl.get_uniform_location(self.program, "uA").as_ref(), 0);
                gl.uniform_1_i32(gl.get_uniform_location(self.program, "uPass").as_ref(), 2);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                gl.bind_vertex_array(None);
            }
        }
    }
    pub fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.program);
            gl.delete_vertex_array(self.vao);
            for t in self.tex {
                gl.delete_texture(t);
            }
            for f in self.fbo {
                gl.delete_framebuffer(f);
            }
        }
    }
    pub fn render(
        &mut self,
        gl: &glow::Context,
        a: glow::Texture,
        b: Option<(glow::Texture, f32, glow::Texture)>,
        spec: glow::Texture,
        u: &crate::glstage::Uniforms,
        size: (i32, i32),
        vp: [i32; 4],
        dt: f32,
    ) {
        unsafe {
            if b.is_none() && u.global_pulse[0] <= 0.0 {
                self.primed = false;
                self.last = Some(a);
                return;
            }
            let target = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            let mut clip = [0; 4];
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut clip);
            gl.disable(glow::SCISSOR_TEST);
            gl.disable(glow::BLEND);
            if self.size != size {
                self.size = size;
                self.primed = false;
                for i in 0..3 {
                    gl.bind_texture(glow::TEXTURE_2D, Some(self.tex[i]));
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MIN_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_MAG_FILTER,
                        glow::LINEAR as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_S,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.tex_parameter_i32(
                        glow::TEXTURE_2D,
                        glow::TEXTURE_WRAP_T,
                        glow::CLAMP_TO_EDGE as i32,
                    );
                    gl.tex_image_2d(
                        glow::TEXTURE_2D,
                        0,
                        glow::RGBA8 as i32,
                        size.0,
                        size.1,
                        0,
                        glow::RGBA,
                        glow::UNSIGNED_BYTE,
                        glow::PixelUnpackData::Slice(None),
                    );
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[i]));
                    gl.framebuffer_texture_2d(
                        glow::FRAMEBUFFER,
                        glow::COLOR_ATTACHMENT0,
                        glow::TEXTURE_2D,
                        Some(self.tex[i]),
                        0,
                    );
                }
            }
            if self.epoch != u.epoch {
                self.epoch = u.epoch;
                self.primed = false;
            }
            gl.use_program(Some(self.program));
            gl.bind_vertex_array(Some(self.vao));
            let loc = |s: &str| gl.get_uniform_location(self.program, s);
            for (name, unit) in [
                ("uA", 0),
                ("uB", 1),
                ("uPrev", 2),
                ("uSpec", 3),
                ("uOtherSpec", 4),
            ] {
                gl.uniform_1_i32(loc(name).as_ref(), unit);
            }
            gl.uniform_1_i32(loc("uBlend").as_ref(), u.blend as i32);
            gl.uniform_1_i32(loc("uReverse").as_ref(), u.blend_reverse as i32);
            gl.uniform_1_i32(loc("uInteraction").as_ref(), u.interaction_mode as i32);
            gl.uniform_1_i32(loc("uDirection").as_ref(), u.interaction_direction as i32);
            gl.uniform_4_f32(loc("uInteractionControls").as_ref(), u.interaction[0], u.interaction[1], u.interaction[2], u.interaction[3]);
            gl.uniform_1_f32(loc("uDt").as_ref(), dt.clamp(0.0, 0.1));
            gl.uniform_4_f32(
                loc("uPulse").as_ref(),
                u.global_pulse[0],
                u.global_pulse[1],
                u.global_pulse[2],
                u.global_pulse[3],
            );
            let bind = |unit, tex| {
                gl.active_texture(glow::TEXTURE0 + unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            };
            bind(3, spec);
            let mut source = a;
            gl.viewport(0, 0, size.0, size.1);
            if let Some((other, mix, other_spec)) = b {
                bind(4, other_spec);
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[0]));
                bind(0, a);
                bind(1, other);
                gl.uniform_1_i32(loc("uPass").as_ref(), 0);
                gl.uniform_1_f32(loc("uMix").as_ref(), mix);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                source = self.tex[0];
            }
            if u.global_pulse[0] > 0.0 {
                let next = 1 - self.ping;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo[1 + next]));
                bind(0, source);
                bind(2, self.tex[1 + self.ping]);
                gl.uniform_1_i32(loc("uPass").as_ref(), 1);
                gl.uniform_1_i32(loc("uFirst").as_ref(), (!self.primed) as i32);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
                self.ping = next;
                self.primed = true;
                source = self.tex[1 + next];
            } else {
                self.primed = false;
            }
            self.last = Some(source);
            gl.bind_framebuffer(
                glow::FRAMEBUFFER,
                std::num::NonZeroU32::new(target as u32).map(glow::NativeFramebuffer),
            );
            gl.viewport(vp[0], vp[1], vp[2], vp[3]);
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(clip[0], clip[1], clip[2], clip[3]);
            bind(0, source);
            gl.uniform_1_i32(loc("uPass").as_ref(), 2);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.bind_vertex_array(None);
        }
    }
}
