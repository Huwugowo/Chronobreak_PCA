"""Read-only same-clock resource detail; retains original analyzer semantics."""
import argparse
import json
from pathlib import Path
import statistics

parser=argparse.ArgumentParser()
parser.add_argument('--order',type=Path,required=True)
parser.add_argument('--valid-count',type=int,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
order=json.loads(args.order.read_text())
rows=[]
for launch in order['launches'][:args.valid_count]:
    root=Path(launch['result_root'])
    def lines(name): return [json.loads(line) for line in (root/name).read_text().splitlines() if line.strip()]
    samples=lines('process_samples.jsonl')
    observations=lines('observer.jsonl')
    requests=lines('server_requests.jsonl')
    events=lines('events.jsonl')
    row={k:launch[k] for k in ('sequence','arm','cell','kind','trial')}
    # Never subtract frontend and server/native/collector clocks.
    frontend={x['kind']:x['monotonic_ms'] for x in events if x['source']=='frontend' and x['kind'] in ('games_library_usable','library_useful')}
    row['frontend_games_to_historical_useful_ms']=frontend['library_useful']-frontend['games_library_usable']
    row['server_routes']={}
    for route in sorted({x['route_class'] for x in requests}):
        matches=[x for x in requests if x['route_class']==route]
        row['server_routes'][route]=dict(count=len(matches),delivered_bytes=sum(x['delivered_bytes'] for x in matches),
            errors=sum(x['outcome']=='error' for x in matches),cancelled=sum(x['outcome']=='cancelled' for x in matches),
            completed=sum(x['outcome']=='completed' for x in matches),
            server_clock_span_ms=max(x['completed_ms'] for x in matches)-min(x['started_ms'] for x in matches))
    row['collector_sample_count']=len(samples)
    row['whole_system_cpu_mean']=statistics.fmean(x['whole_system_cpu_percent'] for x in samples)
    row['operations_sample_delta']={k:samples[-1][k]-samples[0][k] for k in ('io_read_operations','io_write_operations','io_other_operations')}
    row['growth_signals']={}
    for name in ('process_tree_private_bytes','working_set_bytes','handles','threads'):
        values=[x[name] for x in samples]
        row['growth_signals'][name]=dict(first=values[0],last=values[-1],peak=max(values),
            monotonic=all(b>=a for a,b in zip(values,values[1:])) and values[-1]>values[0],
            latter_half_growth=values[-1]-values[len(values)//2])
    row['process_class_series']={}
    for name in ('league-replay-app.exe','msedgewebview2.exe','ffprobe.exe'):
        series=[]
        for obs in observations:
            matches=[p for p in obs['process_tree'] if p['name'].lower()==name]
            series.append(dict(collector_ms=obs['monotonic_ms'],count=len(matches),
                private_bytes=sum(p['private_bytes'] for p in matches),
                working_set_bytes=sum(p['working_set_bytes'] for p in matches)))
        row['process_class_series'][name]=series
    rows.append(row)
value=dict(policy='Diagnostic same-clock extraction; original full-window analyzer metrics unchanged. Operation deltas use first/last core samples like existing byte deltas. No native/frontend/collector clock subtraction.',runs=rows)
with args.output.open('x',encoding='utf-8') as stream: json.dump(value,stream,indent=2); stream.write('\n')
for row in rows:
    flags=[k for k,v in row['growth_signals'].items() if v['monotonic']]
    print(row['sequence'],row['arm'],row['cell'],row['kind'],'detail gap ms',round(row['frontend_games_to_historical_useful_ms'],1),'growth',flags)
