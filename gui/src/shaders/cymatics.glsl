// Stylized plate modes, continuously mixed by bands rather than a physics solver.
void main(){
    vec2 p=aspect(vUv)*2.15;
    float angle=uTime*.035*uControls.w+uWidth*.16;
    p=mat2(cos(angle),-sin(angle),sin(angle),cos(angle))*p;
    float radius=length(p);
    float n=2.+3.*uControls.x;
    float m=3.+floor(spec(.55)*4.);
    float drift=sin(uTime*.19*uControls.w)*.4;
    float a=cos(n*3.14159*p.x)*cos(m*3.14159*p.y)-cos(m*3.14159*p.x)*cos(n*3.14159*p.y);
    float b=sin((n+2.)*3.14159*p.x+drift)*sin((m-1.)*3.14159*p.y);
    float field=a*(.6+uBass*.5)+b*(.12+uMid*.45)+sin(radius*24.-uTime*.2)*uTreb*.13;
    float sharp=18.+35.*uControls.y;
    float line=exp(-abs(field)*sharp);
    float halo=exp(-abs(field)*6.)*.22;
    float grain=mix(1.,.45+hash(floor(gl_FragCoord.xy*.8))*1.2,clamp(uControls.z*.6,0.,1.));
    vec3 ink=uRainbow==1?hsv2rgb(vec3(.47+radius*.18+uBright*.10,.65,.9)):uAccent;
    vec3 col=ink*(line*grain*(1.0+uPulse*.6)+halo);
    col+=vec3(.25,.3,.3)*line*line;
    col*=1.-smoothstep(.93,1.01,radius);
    float rim=exp(-abs(radius-.99)*260.);
    col+=ink*rim*.45;
    col+=ink*.008*(1.-smoothstep(.9,1.,radius));
    frag=vec4(col,1.);
}
