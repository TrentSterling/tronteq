// A frequency-history district: columns are frequencies; depth is elapsed audio.
float tower(vec3 p) {
    vec2 q=p.xz+vec2(0.,uTime*.45);
    vec2 cell=floor(q/1.2);
    vec2 local=mod(q,1.2)-.6;
    float frequency=clamp((cell.x+10.)/20.,0.,1.);
    float energy=hist(vec2(frequency,clamp(p.z/25.,0.,.94)));
    float height=.15+pow(clamp(energy,0.,1.),1.8)*5.5*uControls.x;
    if(abs(cell.x)<1.0)height=.015;
    vec3 d=abs(vec3(local.x,p.y-height*.5,local.y))-vec3(.36*clamp(uControls.y,.3,1.5),height*.5,.38);
    return length(max(d,0.))+min(max(d.x,max(d.y,d.z)),0.);
}
void main(){
    vec2 uv=aspect(vUv)*2.;
    vec3 ro=vec3(sin(uTime*.11)*1.4,3.3+uControls.w*1.4,-5.);
    vec3 forward=normalize(vec3(0.,1.1,9.)-ro);
    vec3 right=normalize(cross(forward,vec3(0,1,0)));
    vec3 up=cross(right,forward);
    vec3 rd=normalize(forward*1.4+right*uv.x+up*uv.y);
    vec3 col=vec3(.007,.009,.025)+vec3(.04,.015,.075)*exp(-abs(rd.y)*6.);
    float t=0.;bool hit=false;vec3 p=ro;
    for(int i=0;i<108;i++){p=ro+rd*t;float d=min(p.y,tower(p));if(d<.008){hit=true;break;}t+=clamp(d,.015,.48);if(t>36.)break;}
    if(hit){
        vec2 block=mod(p.xz+vec2(0.,uTime*.45),1.2)-.6;
        float hue=fract(.52+p.x*.022+p.z*.006);
        vec3 ink=uRainbow==1?hsv2rgb(vec3(hue,.8,1.)):uAccent;
        float edge=1.-smoothstep(.01,.035,min(abs(abs(block.x)-.36*clamp(uControls.y,.3,1.5)),abs(abs(block.y)-.38)));
        vec3 e=vec3(.012,0,0);
        vec3 normal=normalize(vec3(tower(p+e.xyy)-tower(p-e.xyy),tower(p+e.yxy)-tower(p-e.yxy),tower(p+e.yyx)-tower(p-e.yyx))+vec3(.00001));
        vec2 glass=fract(vec2(p.y*3.5,(p.x+p.z)*7.));
        vec2 aa=max(fwidth(vec2(p.y*3.5,(p.x+p.z)*7.)),vec2(.035));
        float windows=smoothstep(.34-aa.x,.34+aa.x,glass.x)*(1.-smoothstep(.91-aa.x,.91+aa.x,glass.x))*smoothstep(.38-aa.y,.38+aa.y,glass.y)*(1.-smoothstep(.88-aa.y,.88+aa.y,glass.y));
        float lighting=.12+windows*.62*uControls.z+edge*.3;
        col=ink*mix(lighting,.2,clamp(normal.y,0.,1.));
        if(p.y<.03){vec2 grid=abs(fract((p.xz+vec2(0,uTime*.45))/1.2)-.5);float line=1.-smoothstep(.012,.025,min(grid.x,grid.y));col=ink*(.025+line*.38);float road=exp(-abs(p.x)*1.6);col+=vec3(.16,.045,.22)*road*(.2+uPulse);}
        col=mix(vec3(.01,.013,.03),col,exp(-t*.055));
    }
    col*=1.-.3*dot(vUv-.5,vUv-.5);
    frag=vec4(col,1.);
}
