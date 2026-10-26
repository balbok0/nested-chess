import { DatabaseSync } from 'node:sqlite';
import { readFileSync, writeFileSync } from 'node:fs';
const [source,output,...extra]=process.argv.slice(2);
if (!source||!output||extra.length) throw new Error('Usage: node scripts/export-sqlite.mjs OLD.sqlite EXPORT.json');
const db=new DatabaseSync(source,{readOnly:true});
const version=db.prepare('PRAGMA user_version').get().user_version;
if (![1,2].includes(version)) throw new Error('Expected online SQLite schema version 1 or 2.');
const spec=JSON.parse(readFileSync(new URL('../crates/server/legacy-tables.json',import.meta.url),'utf8'));
const tables={};
db.exec('BEGIN');
for(const [table,columns] of Object.entries(spec)) {
  if(version===1&&['mobile_auth','stream_tickets'].includes(table)) {tables[table]=[];continue;}
  const select=columns.split(',').filter(c=>!['checkpoint','deadline_ms'].includes(c)).map(c=>c==='ordinal'?'rowid AS ordinal':c).join(',');
  tables[table]=db.prepare(`SELECT ${select} FROM ${table}`).all().map(row=>{
    for(const key of ['guest','linking','rated','ready_white','ready_black']) if(key in row) row[key]=Boolean(row[key]);
    return row;
  });
}
db.exec('COMMIT');db.close();
writeFileSync(output,JSON.stringify({version:1,tables}),{flag:'wx',mode:0o600});
console.log('Exported. Keep this file private: it contains accounts and session hashes.');
