// Reproducible RGB pixels for UI import/zoom/export checks, no local QA dependency.
const zlib=require('node:zlib');
function imageFixture(width=1024,height=1024){
 const pixels=Buffer.alloc((width*3+1)*height);
 for(let y=0;y<height;y++)for(let x=0;x<width;x++){
  const i=y*(width*3+1)+1+x*3,subject=x>width*.28&&x<width*.72&&y>height*.2&&y<height*.8;
  const rgb=subject?[190,60+(y%35),55+(x%30)]:[125+(x%35),155+(y%40),180];
  rgb.forEach((value,channel)=>pixels[i+channel]=value);
 }
 const crc=buffer=>{let value=0xffffffff;for(const byte of buffer){value^=byte;for(let n=0;n<8;n++)value=(value>>>1)^((value&1)?0xedb88320:0);}return(value^0xffffffff)>>>0;};
 const chunk=(name,body)=>{const type=Buffer.from(name),head=Buffer.alloc(4),tail=Buffer.alloc(4);head.writeUInt32BE(body.length);tail.writeUInt32BE(crc(Buffer.concat([type,body])));return Buffer.concat([head,type,body,tail]);};
 const header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(height,4);header[8]=8;header[9]=2;
 return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',header),chunk('IDAT',zlib.deflateSync(pixels)),chunk('IEND',Buffer.alloc(0))]);
}
module.exports={imageFixture};
