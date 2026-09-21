"""Small gate summary from final retained reports, including independent warm medians."""
import json
from pathlib import Path
import statistics as s

repo=Path(__file__).resolve().parents[2]
out=repo/'docs/performance/evidence/qb-replay-010-m4-replacement-20260917'
comparison=json.loads((out/'comparison.json').read_text())
opening=json.loads((out/'opening.json').read_text())
resources=json.loads((out/'resource-details.json').read_text())['runs']
def band(values,floor):
    median=s.median(values)
    return max(floor,3*s.median(abs(x-median) for x in values))
summary={'primary':[],'warm_remounts':[],'operation_gates':[],'individual_growth_flags':[],'resource_table':[],
    'disposition_metrics':[r for r in comparison['metrics'] if r['trial_statistic']=='median' and r['disposition_requiring']]}
for cell,kind in [('s','cold'),('n','cold'),('e','cold'),('n','warm'),('e','warm')]:
    scenario=f'qb010-library-v2-{cell}-{kind}'
    metrics={r['metric']:r for r in comparison['metrics'] if r['scenario_id']==scenario and r['trial_statistic']=='median'}
    primary=metrics['library_request_to_games_usable_ms']
    improvement=primary['baseline_median']-primary['current_median']
    summary['primary'].append(dict(cell=cell,kind=kind,reference_ms=primary['baseline_median'],candidate_ms=primary['current_median'],
        improvement_percent=100*improvement/primary['baseline_median'],band_ms=primary['repeatability_band'],
        eligible=primary['eligible'],improvement_gate_pass=primary['eligible'] and improvement>primary['repeatability_band'] and improvement/primary['baseline_median']>0.05,
        regression_requires_disposition=primary['disposition_requiring']))
    selected=['library_request_to_useful_ms','process_tree_cpu_percent_mean','process_tree_cpu_time_100ns_delta',
        'process_tree_private_bytes_max','working_set_bytes_max','io_read_bytes_delta','io_write_bytes_delta',
        'server_requests_count','server_delivered_bytes','server_error_count','server_cancelled_count','whole_system_cpu_percent_mean']
    summary['resource_table'].append(dict(cell=cell,kind=kind,metrics={m:{k:metrics[m][k] for k in
        ('baseline_median','current_median','repeatability_band','disposition_requiring')} for m in selected if m in metrics}))
    group=[r for r in resources if r['cell']==cell and r['kind']==kind]
    for metric in ('io_read_operations','io_write_operations','io_other_operations'):
        before=[r['operations_sample_delta'][metric] for r in group if r['arm']=='reference']
        after=[r['operations_sample_delta'][metric] for r in group if r['arm']=='candidate']
        b,c=s.median(before),s.median(after); tolerance=band(before,max(1,abs(b)*0.01))
        summary['operation_gates'].append(dict(cell=cell,kind=kind,metric=metric,reference=b,candidate=c,
            band=tolerance,disposition_requiring=len(before)==len(after)==5 and c-b>tolerance and c-b>abs(b)*.05))
    for row in group:
        for metric,value in row['growth_signals'].items():
            if value['monotonic']:
                summary['individual_growth_flags'].append(dict(sequence=row['sequence'],cell=cell,kind=kind,arm=row['arm'],metric=metric,**value))
    if kind=='warm':
        rows=[r for r in opening if r['cell']==cell and r['kind']==kind]
        for phase in ('initial_mount','warm_remount_process_medians'):
            for metric in rows[0][phase]:
                if not metric.endswith('_ms'): continue
                before=[r[phase][metric] for r in rows if r['arm']=='reference']
                after=[r[phase][metric] for r in rows if r['arm']=='candidate']
                b,c=s.median(before),s.median(after); tolerance=band(before,5)
                summary['warm_remounts'].append(dict(cell=cell,phase=phase,metric=metric,reference_ms=b,candidate_ms=c,
                    band_ms=tolerance,disposition_requiring=len(before)==len(after)==5 and c-b>tolerance and c-b>abs(b)*.05))
with (out/'gate-summary.json').open('x',encoding='utf-8') as stream: json.dump(summary,stream,indent=2); stream.write('\n')
print('PRIMARY',json.dumps(summary['primary']))
print('DISPOSITIONS',[(r['scenario_id'],r['metric'],round(r['baseline_median'],3),round(r['current_median'],3)) for r in summary['disposition_metrics']])
print('GROWTH',json.dumps(summary['individual_growth_flags']))
