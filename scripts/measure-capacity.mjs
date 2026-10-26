import { spawn,execFile } from 'node:child_process';
import { mkdirSync,writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
const [url,games,seconds,rate,warmup,concurrency,output,project='nested-chess']=process.argv.slice(2);
if(!output)throw new Error('Usage: node scripts/measure-capacity.mjs URL GAMES SECONDS RATE WARMUP CONCURRENCY OUTPUT.json [COMPOSE_PROJECT]');
mkdirSync(dirname(output),{recursive:true});
const samples=[];let sampling=false;let phase='setup';
const sample=()=>{
  if(sampling)return;sampling=true;
  execFile('docker',['stats','--no-stream','--format','{{json .}}',...['app-a','app-b','db','proxy'].map(s=>`${project}-${s}-1`)],{timeout:10_000},(error,stdout)=>{
    sampling=false;
    if(error){console.error(error.message);return;}
    samples.push({time:new Date().toISOString(),phase,containers:stdout.trim().split('\n').map(line=>JSON.parse(line))});
  });
};
sample();const timer=setInterval(sample,5000);
const child=spawn('target/release/examples/capacity',[url,games,seconds,rate,warmup,concurrency,output],{stdio:['ignore','inherit','pipe']});
child.stderr.on('data',chunk=>{process.stderr.write(chunk);if(chunk.toString().includes('sockets ready'))phase='measured';});
const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('exit',resolve);});
clearInterval(timer);
while(sampling)await new Promise(resolve=>setTimeout(resolve,50));
writeFileSync(output.replace(/\.json$/,'.resources.json'),JSON.stringify(samples,null,2));
if(code!==0)throw new Error(`Capacity run failed with exit ${code}`);
console.log(`Saved ${samples.length} resource samples beside ${output}`);
