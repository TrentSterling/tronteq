// The two channels modulate separate orbital ribbons and their travelling heads.
void main(){
    vec2 p=aspect(vUv)*2.;vec3 col=vec3(0.);
    for(int channel=0;channel<2;channel++){
        float side=channel==0?-1.:1.;
        float turn=side*(.32+.35*uControls.z)+uTime*.1;
        vec2 q=mat2(cos(turn),-sin(turn),sin(turn),cos(turn))*p;
        q.x-=side*uWidth*.14;
        q.y/=.62;
        float a=atan(q.y,q.x);float x=fract(a/6.2831853+.5);
        vec2 sampleLR=texture(uStereo,vec2(x,.5)).rg;
        float wave=channel==0?sampleLR.x:sampleLR.y;
        float r=.72*uControls.x+.14*uBass+wave*.26*uControls.y;
        float d=abs(length(q)-r);
        float core=exp(-d*420.);float glow=exp(-d*55.);
        vec3 ink=uRainbow==1?hsv2rgb(vec3(fract(.52+float(channel)*.31+x*.1),.76,1.)):mix(uAccent,uAccent.brg,float(channel)*.5);
        col+=ink*(core*.95+glow*.28);
        float head=exp(-pow(sin((a-uTime*(.7+float(channel)*.25))* .5)*24.,2.));
        col+=mix(ink,vec3(1),.6)*glow*head*(.4+uPulse);
    }
    vec2 trail=(vUv-.5)*1.002+.5;
    vec3 prev=texture(uPrev,trail).rgb;
    float persistence=clamp(.80+uControls.w*.075,.0,.965);
    col=max(col,prev*persistence);
    frag=vec4(col,1.);
}
