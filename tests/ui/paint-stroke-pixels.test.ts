import test from 'node:test';
import assert from 'node:assert/strict';
import { assertContinuousPaint } from '../native/paint-stroke-pixels.mjs';
const width=160, height=80, start={x:30,y:40}, end={x:130,y:40};
const frame=()=>({pixels:new Uint8Array(width*height*3)});
function dot(f, x, radius=20) {
    for(let py=0;py<height;py++) for(let px=0;px<width;px++) {
        if(Math.hypot(px-x,py-40)<=radius) {
            const offset=(py*width+px)*3; f.pixels[offset]=255;f.pixels[offset+2]=255;
        }
    }
    return f;
}
test('native paint oracle rejects a single endpoint dab',()=>{
    assert.throws(()=>assertContinuousPaint(frame(),dot(frame(),end.x),width,height,start,end),/not continuously visible/);
});
test('native paint oracle rejects isolated start and end dabs',()=>{
    assert.throws(()=>assertContinuousPaint(frame(),dot(dot(frame(),start.x),end.x),width,height,start,end),/not continuously visible/);
});
test('native paint oracle rejects unchanged magenta and unrelated viewport changes',()=>{
    const before=dot(frame(),end.x),after=dot(frame(),10);
    assert.throws(()=>assertContinuousPaint(before,before,width,height,start,end),/not continuously visible/);
    assert.throws(()=>assertContinuousPaint(frame(),after,width,height,start,end),/not continuously visible/);
});
test('native paint oracle accepts the complete visibly changed path',()=>{
    const after=frame();for(let x=start.x;x<=end.x;x+=3)dot(after,x,4);
    assert.equal(assertContinuousPaint(frame(),after,width,height,start,end).covered_samples,26);
});
