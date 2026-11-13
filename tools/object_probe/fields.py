names=[l.strip()[6:] for l in reversed(open(__import__('os').path.join(__import__('os').path.dirname(__file__) if '__file__' in globals() else '.', 'obj_field_names_reversed.txt')).read().split())]
def ftype(n):
    if n in ('location','teleport_dest','critter_teleport_dest'): return 'i64'
    if n in ('critter_fleeing_from','critter_gold','critter_arrows_obj'): return 'oid'
    if n=='pc_player_name': return 'str'
    if n in ('pc_global_flags','pc_global_variables'): return 'sa'
    if n.endswith('_idx') or n.endswith('ias_1') or n.endswith('ias_2') or n.endswith('i64as_1') or n in ('overlay_fore','overlay_back','underlay') or n.startswith('overlay_light_'): return 'sa'
    return 'i32'
def block(begin,end):
    i=names.index(begin); j=names.index(end); return names[i+1:j]
PC=[('base',0,block('begin','end')),('critter',64,block('critter_begin','critter_end')),('pc',128,block('pc_begin','pc_end'))]
