vec2 scopePoint(int i) {
    vec2 lr=texelFetch(uStereo,ivec2(i,0),0).rg;
    // Vertical = correlated centre; horizontal = out-of-phase side content.
    vec2 xy=vec2(lr.x-lr.y,lr.x+lr.y)*.5;
    return xy*(.25+uControls.x*2.5);
}
vec2 ribbonPoint(int i, float layer) {
    float t=float(i)/255.;
    // Sculpture mode deliberately forms a ribbon; the separate Meter variant
    // retains the unmodified mid/side measurement geometry.
    vec2 sampleXY=scopePoint(max(i-2,0))*.25+scopePoint(i)*.5+scopePoint(min(i+2,255))*.25;
    vec2 xy=vec2(cos(t*6.283)*(.17+uWidth*.12),sin(t*6.283)*.29);
    xy+=tanh(sampleXY)*.14;
    float a=uTime*.18*uDetails.y + layer*.7;
    float z=sin(t*6.283+layer*1.6+uTime*.11)*.14*uDetails.x;
    float twist=(t-.5)*uDetails.w*.8;
    xy=mat2(cos(twist),-sin(twist),sin(twist),cos(twist))*xy;
    vec3 p=vec3(xy.x*cos(a)+z*sin(a),xy.y, -xy.x*sin(a)+z*cos(a));
    return p.xy/(1.1+p.z)*1.15;
}
float ribbonSegment(vec2 p, vec2 a, vec2 b) {
    vec2 v=b-a;
    return length(p-a-v*clamp(dot(p-a,v)/max(dot(v,v),.0000001),0.,1.));
}
void main() {
    vec2 p=aspect(vUv);
    float r=length(p), px=1./uRes.y;
    if (uVariant == 1) {
        vec3 col=vec3(0.);
        float thickness=(.55+uControls.y*1.4)*px;
        for(int layer=0;layer<3;++layer) {
            float distanceTo=10.;
            vec2 a=ribbonPoint(0,float(layer));
            for(int j=3;j<256;j+=3) {
                vec2 b=ribbonPoint(j,float(layer));
                distanceTo=min(distanceTo,ribbonSegment(p,a,b)); a=b;
            }
            vec3 tint=uRainbow==1?hsv2rgb(vec3(.51+float(layer)*.13+uWidth*.15,.75,1.)):uAccent;
            float core=exp(-distanceTo/thickness);
            float glow=exp(-distanceTo/(thickness*8.))*.12*uDetails.z;
            col+=tint*(core+glow)*(uLoud*.7+uPulse*.3);
        }
        float halo=exp(-abs(r-.38)/(.002+px))*uControls.z*.035;
        col+=uAccent*halo;
        vec3 old=texture(uPrev,(vUv-.5)/1.002+.5).rgb*mix(.55,.965,uControls.w*.5);
        col=max(col,max(old-.004,vec3(0.)));
        frag=vec4(col,1.); return;
    }
    float grid=exp(-abs(r-.42)/(px*1.1))*.15;
    grid+=exp(-abs(r-.21)/(px*.8))*.055;
    grid+=(exp(-abs(p.x)/(px*.7))+exp(-abs(p.y)/(px*.7)))*.045;
    grid+=(exp(-abs(p.x-p.y)/(px*.9))+exp(-abs(p.x+p.y)/(px*.9)))*.04;
    vec3 gridCol=mix(vec3(.15,.25,.32),uAccent,.4)*grid*uControls.z;
    float line=0.;
    float width=(.5+uControls.y*1.5)*px;
    for(int i=0;i<256;++i) {
        // Unconnected samples avoid inventing straight lines between
        // decimated points. Persistence accumulates the true stereo cloud.
        float distanceTo=length(p-scopePoint(i));
        line=max(line,exp(-distanceTo/(width*1.3))*(0.55+float(i)/256.*.45));
    }
    vec3 tint=uRainbow==1?hsv2rgb(vec3(.52+atan(p.y,p.x)/6.283*.16,.7,1.)):uAccent;
    vec3 trace=tint*line*(.5+uLoud*.6);
    // Decay without blurring or rotating the true stereo geometry.
    vec3 old=texture(uPrev,vUv).rgb*mix(.80,.985,uControls.w*.5);
    trace=max(trace,max(old-.006,vec3(0.)));
    float border=1.-smoothstep(.43,.46,r);
    frag=vec4(max(trace,gridCol)*border,1.);
}
