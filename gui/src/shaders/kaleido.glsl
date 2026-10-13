// Fold an asymmetric, spectrum-lit lattice into a true mirrored rosette.
void main() {
    vec2 p=aspect(vUv);
    float radius=length(p);
    float sectors=4.+floor(uControls.x*4.);
    float wedge=6.2831853/sectors;
    float angle=atan(p.y,p.x)+uTime*.055+uWidth*.12;
    float folded=abs(mod(angle+wedge*.5,wedge)-wedge*.5);
    vec2 q=vec2(cos(folded),sin(folded))*radius;
    float zoom=.85+uControls.y*.55+uBass*.16;
    q*=zoom;
    q+=vec2(sin(uTime*.13)*.09,cos(uTime*.17)*.055);
    vec3 color=vec3(0.);
    for(int i=0;i<4;i++) {
        float layer=float(i);
        float e=spec(.08+layer*.23);
        vec2 f=q*(3.+layer*1.5)+vec2(layer*.47,uTime*.04);
        f.y+=sin(f.x*2.1+uTime*.2+layer)*(.25+uMid*.3);
        vec2 cell=abs(fract(f)-.5);
        float edge=abs(cell.x+cell.y-(.27+e*.14));
        float width=.016+.020*uControls.z;
        float line=exp(-edge*edge/(width*width));
        float halo=exp(-edge*22.)*.12*uControls.z;
        float gate=.4+.6*smoothstep(-.2,.7,sin(q.x*8.-layer*1.4+uTime*.25));
        vec3 tint=uRainbow==1?hsv2rgb(vec3(fract(.55+layer*.13+radius*.18+uTime*.01),.8,1.)):uAccent;
        color+=tint*(line+halo)*gate*(.16+e*.48+uPulse*.16);
    }
    color=1.-exp(-color*2.2);
    float vignette=1.-smoothstep(.35,1.15,radius);
    color*=vignette;
    vec2 back=p*(1.001+uPulse*.003);back.x/=uRes.x/uRes.y;
    vec3 trail=texture(uPrev,back+.5).rgb*mix(.35,.94,uControls.w*.5);
    frag=vec4(max(color,trail),1.);
}
