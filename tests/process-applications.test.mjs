import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const api = {};
vm.runInNewContext(readFileSync(new URL('../shell/bingux/ProcessApplications.js', import.meta.url), 'utf8'), api);
const service = {byId: () => null, heuristicLookup: () => null};
test('aliases retain identity and duplicate fields do not make one application ambiguous', () => {
 const app = {id: 'foot.desktop', name: 'Foot', startupClass: 'foot', command: ['/usr/bin/foot']};
 assert.equal(api.lookup({name:'FOOT'}, api.index([app]), service), app);
});
test('ambiguous aliases fall back and exact desktop IDs take priority', () => {
 const apps = [{id:'one', name:'Editor'}, {id:'two', name:'Editor'}];
 const aliases = api.index(apps);
 assert.equal(api.lookup({name:'editor'}, aliases, service), null);
 assert.equal(api.lookup({name:'editor'}, aliases, {...service, byId: () => apps[0]}), apps[0]);
 assert.equal(api.lookup({name:'editor'}, aliases, {...service, heuristicLookup: () => apps[1]}), apps[1]);
 assert.equal(api.lookup({name:'editor'}, api.index([apps[0]]), service), apps[0]);
});
test('repeated process lookups do not read application properties again', () => {
 let reads = 0;
 const apps = Array.from({length:1000}, (_, i) => ({get id() { reads++; return 'app' + i; }, name:'App ' + i, command:['/bin/app' + i]}));
 const aliases = api.index(apps);
 const initial = reads;
 for (let i=0; i<10000; i++) assert.equal(api.lookup({name:'app999'}, aliases, service), apps[999]);
 assert.equal(reads, initial);
});
