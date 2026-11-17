import importlib.util,json
from pathlib import Path
repo=Path('/Users/josephwylie/Desktop/Projects/ferrite-remote-connections')
spec=importlib.util.spec_from_file_location('p',repo/'scripts/verify-remote-parity.py');p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
o=repo/'docs/audits/2026-11-16-remote-connections/parity/compact-final'
s=p.prepare(o,repo/'target/debug/ferrite',repo/'target/debug/ferrited');u=p.Window(o,s,'640x420')
try:
 p.BASE.launch(o,s,'640x420');u.wait('board');u.command('key','escape');u.settings();u.shot('01-settings');u.click('connection-add');u.shot('02-remote-link');u.click('connection-mode-ssh');u.shot('03-ssh');u.field('connection-host','fixture.invalid');u.field('connection-port','0');u.click('connection-submit');u.wait('connection-error');u.shot('04-ssh-error')
finally:
 p.BASE.stop_processes(o,s)
 p.BASE.write(o/'summary.json',json.dumps({'owned_processes_stopped':all(p.BASE.identity(i['pid'])!=i['identity'] for i in s['processes']),'screenshots':p.rows(o,'screenshot')},indent=2))
