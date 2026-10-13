#!/usr/bin/env python3
"""Execute the exact shipped C templates, instantiated independently in Python.

This tests the C standard-library implementation, NOT the Rust frontend,
monomorphizer, Rust template substitutions, borrow checker, or full compiler.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
LIST = 'NEW ADD SIZE CAPACITY IS_EMPTY GET GET_REF GET_MUT SET POP REMOVE TRY_GET TRY_POP TRY_REMOVE CLEAR RESERVE'.split()
TEXT = 'NEW APPEND APPEND_INT APPEND_BOOLEAN APPEND_TEXT BYTE_LENGTH BYTE_AT IS_EMPTY CLEAR PRINT CLONE EQUALS EQUALS_STRING PARSE_INT SLICE_BYTES'.split()

def option_code(cid, element):
    return f'''typedef struct ncr_obj_{cid} {{
        int32_t tag;
        union {{ struct {{ unsigned char ncr_empty; }} v0; struct {{ {element} f0; }} v1; }} data;
    }} ncr_obj_{cid};\n'''

def list_code(cid, element, zero, drop='', copy=True, borrow_type='', borrow_element=''):
    option_id = 300 + cid
    fields = f'typedef struct ncr_obj_{cid} {{ {element} *items; int32_t length; int32_t capacity; }} ncr_obj_{cid};\n'
    destructor = f'''static void ncr_drop_{cid}(ncr_obj_{cid} *p) {{
        if (p == NULL) return;
        while (p->length > 0) {{ --p->length; {drop.replace('VALUE','p->items[p->length]')} }}
        ncr_deallocate(p->items); ncr_deallocate(p);
    }}\n'''
    body = (ROOT/'stdlib/list.c.in').read_text()
    body = body.replace('@GET_METHOD@', (ROOT/'stdlib/list_get.c.in').read_text() if copy else '')
    try_get = (ROOT/'stdlib/list_try_get.c.in').read_text() if copy else ''
    try_owned = (ROOT/'stdlib/list_try_owned.c.in').read_text()
    for marker, value in {
        '@OPTION_TYPE@': f'ncr_obj_{option_id}', '@INDEX_TYPE@': 'int32_t',
        '@NONE_TAG@': '0', '@SOME_TAG@': '1',
        '@STRICT_POP_CALL@': f'list{cid}_pop',
        '@STRICT_REMOVE_CALL@': f'list{cid}_remove',
    }.items():
        try_get = try_get.replace(marker, value)
        try_owned = try_owned.replace(marker, value)
    body = body.replace('@TRY_GET_METHOD@', try_get)
    body = body.replace('@TRY_OWNED_METHODS@', try_owned)
    borrowed = ''
    if borrow_type:
        helper = (ROOT/'stdlib/list_get_borrowed.c.in').read_text()
        for name in ('GET_REF', 'GET_MUT'):
            method = helper.replace('@RETURN_TYPE@', borrow_type)
            method = method.replace('@INDEX_TYPE@', 'int32_t')
            method = method.replace('@LIST_METHOD@', f'@LIST_{name}@')
            borrowed += method.replace('@BORROWED_ELEMENT@', borrow_element)
    body = body.replace('@BORROWED_GETTERS@', borrowed)
    replacements = {'@ID@': str(cid), '@ELEMENT@': element, '@ZERO@': zero,
                    '@DROP_AT_INDEX@': drop.replace('VALUE', 'p->items[index]')}
    replacements.update({f'@LIST_{x}@':f'list{cid}_{x.lower()}' for x in LIST})
    for k,v in replacements.items(): body=body.replace(k,v)
    assert not re.search(r'@[A-Z_]+@', body)
    return fields + option_code(option_id, element) + destructor + body

def text_code(cid):
    parse_id, slice_id = cid + 100, cid + 200
    fields=f'typedef struct ncr_obj_{cid} {{ unsigned char *bytes; int32_t length; int32_t capacity; }} ncr_obj_{cid};\n'
    # Independent fixture layouts model the real inline Result payloads.
    fields += f'''typedef struct {{
        int32_t tag;
        union {{ struct {{ int32_t f0; }} v0; struct {{ ncr_obj_{cid} *f0; }} v1; }} data;
    }} ncr_obj_{parse_id};
    typedef struct {{
        int32_t tag;
        union {{ struct {{ ncr_obj_{cid} *f0; }} v0; struct {{ ncr_obj_{cid} *f0; }} v1; }} data;
    }} ncr_obj_{slice_id};
    static void text{cid}_new(ncr_obj_{cid} *, ncr_string, size_t, size_t);
    static ncr_obj_{cid} *text{cid}_from_bytes(const unsigned char *bytes, size_t length, size_t line, size_t column) {{
        ncr_obj_{cid} *text = ncr_allocate(sizeof(ncr_obj_{cid}), line, column);
        text{cid}_new(text, (ncr_string){{bytes, length}}, line, column);
        return text;
    }}
    '''
    for result_id in (parse_id, slice_id):
        fields += f'''static ncr_obj_{result_id} text{cid}_error_{result_id}(const char *message, size_t length, size_t line, size_t column) {{
            ncr_obj_{cid} *error = text{cid}_from_bytes((const unsigned char *)message, length, line, column);
            return (ncr_obj_{result_id}){{.tag = 1, .data.v1.f0 = error}};
        }}
        '''
    destructor=f'static void ncr_drop_{cid}(ncr_obj_{cid} *p) {{ if (p) {{ ncr_deallocate(p->bytes); ncr_deallocate(p); }} }}\n'
    body=(ROOT/'stdlib/text.c.in').read_text().replace('@ID@',str(cid))
    replacements = {
        '@PARSE_RESULT@': str(parse_id), '@PARSE_OK_TAG@': '0',
        '@ERROR_PARSE_RESULT@': f'text{cid}_error_{parse_id}',
        '@SLICE_RESULT@': str(slice_id), '@SLICE_OK_TAG@': '0',
        '@ERROR_SLICE_RESULT@': f'text{cid}_error_{slice_id}',
        '@TEXT_MAKE@': f'text{cid}_from_bytes',
    }
    replacements.update({f'@TEXT_{name}@': f'text{cid}_{name.lower()}' for name in TEXT})
    for marker, value in replacements.items():
        body = body.replace(marker, value)
    assert not re.search(r'@[A-Z_]+@',body)
    return fields+destructor+body

def support():
    return '#define NACRE_MEMORY_STATS 1\n'+(ROOT/'src/runtime.c').read_text()+'''\n#include <assert.h>
static size_t created_objects = 0, destroyed_objects = 0;
typedef struct { int32_t n; } Item;
static Item *item_new(int32_t n) { Item *p=ncr_allocate(sizeof(Item),1,1);p->n=n;++created_objects;return p; }
static void item_drop(Item *p) { if(p){++destroyed_objects;ncr_deallocate(p);} }
typedef struct { int32_t tag; union { struct { Item *f0; } v0; struct { int32_t f0; } v1; } data; } Sum;
static void sum_drop(Sum value) { if(value.tag==0) item_drop(value.data.v0.f0); }
typedef struct { void *data; void (*destroy)(void *); } Iface;
static void erase_drop(void *p) { item_drop((Item *)p); }
static void iface_drop(Iface p) { if(p.data) p.destroy(p.data); }
''' + list_code(0,'int32_t','0') + list_code(1,'bool','false') + list_code(2,'ncr_string','(ncr_string){NULL,0}') \
    +list_code(3,'Item *','NULL','item_drop(VALUE);',False,'Item *','p->items[index]') \
    +list_code(4,'Iface','(Iface){NULL,NULL}','iface_drop(VALUE);',False,'Iface','p->items[index]') \
    +list_code(5,'Sum','(Sum){.tag=-1}','sum_drop(VALUE);',False,'Sum *','&p->items[index]') \
    +text_code(10)+list_code(11,'ncr_obj_10 *','NULL','ncr_drop_10(VALUE);',False,'ncr_obj_10 *','p->items[index]') \
    +list_code(12,'ncr_obj_0 *','NULL','ncr_drop_0(VALUE);',False,'ncr_obj_0 *','p->items[index]')

POSITIVE = r'''
static uint32_t seed=71;
static uint32_t random32(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}
#define NEW_LIST(ID,NAME) ncr_obj_##ID *NAME=ncr_allocate(sizeof(ncr_obj_##ID),1,1);list##ID##_new(NAME,1,1)
#define STR(S) (ncr_string){(const unsigned char *)(S),sizeof(S)-1}
int main(void) {
    NEW_LIST(0, xs);
    int32_t reference[4096];int32_t n=0;
    for(int32_t step=0;step<50000;++step){
        uint32_t op=random32()%7;
        if(op==0 && n<4096){int32_t v=(int32_t)(random32()%100000);reference[n++]=v;list0_add(xs,v,1,1);}
        else if(op==1 && n>0){assert(list0_pop(xs,1,1)==reference[--n]);}
        else if(op==2 && n>0){int32_t i=(int32_t)(random32()%(uint32_t)n);assert(list0_remove(xs,i,1,1)==reference[i]);--n;memmove(&reference[i],&reference[i+1],(size_t)(n-i)*sizeof(int32_t));}
        else if(op==3 && n>0){int32_t i=(int32_t)(random32()%(uint32_t)n);int32_t v=(int32_t)(random32()%100000);reference[i]=v;list0_set(xs,i,v,1,1);}
        else if(op==4){list0_reserve(xs,(int32_t)(random32()%4097),1,1);}
        else if(op==5 && random32()%17==0){list0_clear(xs,1,1);n=0;}
        assert(list0_size(xs,1,1)==n);assert(list0_capacity(xs,1,1)>=n);assert(list0_is_empty(xs,1,1)==(n==0));
        for(int32_t i=0;i<n;++i)assert(list0_get(xs,i,1,1)==reference[i]);
    }
    ncr_drop_0(xs);
    NEW_LIST(1, bs);list1_add(bs,true,1,1);list1_add(bs,false,1,1);assert(list1_remove(bs,0,1,1));assert(!list1_pop(bs,1,1));ncr_drop_1(bs);
    NEW_LIST(2, ss);list2_add(ss,STR("a\0b"),1,1);ncr_string z=list2_get(ss,0,1,1);assert(z.length==3&&z.data[1]==0);ncr_drop_2(ss);
    NEW_LIST(1, bool_try);
    list1_add(bool_try, true, 1, 1); list1_add(bool_try, false, 1, 1);
    ncr_obj_301 bool_copy = list1_try_get(bool_try, 0, 1, 1);
    assert(bool_copy.tag == 1 && bool_copy.data.v1.f0);
    ncr_drop_1(bool_try);
    NEW_LIST(2, string_try);
    list2_add(string_try, STR("a\0b"), 1, 1);
    ncr_obj_302 string_copy = list2_try_get(string_try, 0, 1, 1);
    assert(string_copy.tag == 1 && string_copy.data.v1.f0.length == 3 && string_copy.data.v1.f0.data[1] == 0);
    ncr_drop_2(string_try);
    NEW_LIST(0, fallible);
    for (int32_t i = 1; i <= 5; ++i) list0_add(fallible, i * 10, 1, 1);
    int32_t fallible_capacity = list0_capacity(fallible, 1, 1);
    size_t allocations_before_try = ncr_allocations;
    ncr_obj_300 absent = list0_try_get(fallible, -1, 1, 1); assert(absent.tag == 0);
    absent = list0_try_get(fallible, 5, 1, 1); assert(absent.tag == 0);
    absent = list0_try_get(fallible, INT32_MAX, 1, 1); assert(absent.tag == 0);
    absent = list0_try_remove(fallible, -1, 1, 1); assert(absent.tag == 0);
    absent = list0_try_remove(fallible, 5, 1, 1); assert(absent.tag == 0);
    absent = list0_try_remove(fallible, INT32_MAX, 1, 1); assert(absent.tag == 0);
    assert(list0_size(fallible, 1, 1) == 5 && list0_capacity(fallible, 1, 1) == fallible_capacity);
    for (int32_t i = 0; i < 5; ++i) assert(list0_get(fallible, i, 1, 1) == (i + 1) * 10);
    ncr_obj_300 removed = list0_try_remove(fallible, 0, 1, 1);
    assert(removed.tag == 1 && removed.data.v1.f0 == 10);
    removed = list0_try_remove(fallible, 1, 1, 1);
    assert(removed.tag == 1 && removed.data.v1.f0 == 30);
    removed = list0_try_remove(fallible, 2, 1, 1);
    assert(removed.tag == 1 && removed.data.v1.f0 == 50);
    assert(list0_size(fallible, 1, 1) == 2 && list0_get(fallible, 0, 1, 1) == 20 && list0_get(fallible, 1, 1, 1) == 40);
    removed = list0_try_pop(fallible, 1, 1); assert(removed.tag == 1 && removed.data.v1.f0 == 40);
    removed = list0_try_pop(fallible, 1, 1); assert(removed.tag == 1 && removed.data.v1.f0 == 20);
    absent = list0_try_pop(fallible, 1, 1); assert(absent.tag == 0);
    absent = list0_try_remove(fallible, 0, 1, 1); assert(absent.tag == 0);
    absent = list0_try_remove(fallible, INT32_MAX, 1, 1); assert(absent.tag == 0);
    assert(list0_capacity(fallible, 1, 1) == fallible_capacity);
    assert(ncr_allocations == allocations_before_try);
    ncr_drop_0(fallible);
    NEW_LIST(3, owned_try);
    for (int32_t i = 0; i < 5; ++i) list3_add(owned_try, item_new(i), 1, 1);
    allocations_before_try = ncr_allocations;
    ncr_obj_303 missing_owned = list3_try_remove(owned_try, INT32_MAX, 1, 1);
    assert(missing_owned.tag == 0 && list3_size(owned_try, 1, 1) == 5);
    ncr_obj_303 transferred = list3_try_remove(owned_try, 0, 1, 1);
    assert(transferred.tag == 1 && transferred.data.v1.f0->n == 0); item_drop(transferred.data.v1.f0);
    transferred = list3_try_remove(owned_try, 1, 1, 1);
    assert(transferred.tag == 1 && transferred.data.v1.f0->n == 2); item_drop(transferred.data.v1.f0);
    transferred = list3_try_remove(owned_try, 2, 1, 1);
    assert(transferred.tag == 1 && transferred.data.v1.f0->n == 4); item_drop(transferred.data.v1.f0);
    transferred = list3_try_pop(owned_try, 1, 1);
    assert(transferred.tag == 1 && transferred.data.v1.f0->n == 3); item_drop(transferred.data.v1.f0);
    transferred = list3_try_pop(owned_try, 1, 1);
    assert(transferred.tag == 1 && transferred.data.v1.f0->n == 1); item_drop(transferred.data.v1.f0);
    missing_owned = list3_try_pop(owned_try, 1, 1); assert(missing_owned.tag == 0);
    assert(ncr_allocations == allocations_before_try);
    ncr_drop_3(owned_try);
    NEW_LIST(5, enum_try);
    for (int32_t i = 0; i < 5; ++i) list5_add(enum_try, (Sum){.tag=0, .data.v0.f0=item_new(i)}, 1, 1);
    ncr_obj_305 enum_result = list5_try_remove(enum_try, 0, 1, 1);
    assert(enum_result.tag == 1 && enum_result.data.v1.f0.tag == 0 && enum_result.data.v1.f0.data.v0.f0->n == 0);
    sum_drop(enum_result.data.v1.f0);
    enum_result = list5_try_remove(enum_try, 1, 1, 1);
    assert(enum_result.tag == 1 && enum_result.data.v1.f0.data.v0.f0->n == 2);
    sum_drop(enum_result.data.v1.f0);
    enum_result = list5_try_remove(enum_try, 2, 1, 1);
    assert(enum_result.tag == 1 && enum_result.data.v1.f0.data.v0.f0->n == 4);
    sum_drop(enum_result.data.v1.f0);
    enum_result = list5_try_pop(enum_try, 1, 1);
    assert(enum_result.tag == 1 && enum_result.data.v1.f0.data.v0.f0->n == 3);
    sum_drop(enum_result.data.v1.f0);
    ncr_drop_5(enum_try);
    NEW_LIST(4, iface_try);
    for (int32_t i = 0; i < 5; ++i) list4_add(iface_try, (Iface){item_new(i), erase_drop}, 1, 1);
    ncr_obj_304 iface_result = list4_try_remove(iface_try, 0, 1, 1);
    assert(iface_result.tag == 1 && ((Item *)iface_result.data.v1.f0.data)->n == 0);
    iface_drop(iface_result.data.v1.f0);
    iface_result = list4_try_remove(iface_try, 1, 1, 1);
    assert(iface_result.tag == 1 && ((Item *)iface_result.data.v1.f0.data)->n == 2);
    iface_drop(iface_result.data.v1.f0);
    iface_result = list4_try_remove(iface_try, 2, 1, 1);
    assert(iface_result.tag == 1 && ((Item *)iface_result.data.v1.f0.data)->n == 4);
    iface_drop(iface_result.data.v1.f0);
    iface_result = list4_try_pop(iface_try, 1, 1);
    assert(iface_result.tag == 1 && ((Item *)iface_result.data.v1.f0.data)->n == 3);
    iface_drop(iface_result.data.v1.f0);
    ncr_drop_4(iface_try);
    NEW_LIST(3, owned);
    for(int32_t i=0;i<10000;++i)list3_add(owned,item_new(i),1,1);
    for(int32_t i=9999;i>=9000;--i){Item *p=list3_pop(owned,1,1);assert(p->n==i);item_drop(p);}
    for(int32_t i=0;i<1000;++i){Item *p=list3_remove(owned,0,1,1);assert(p->n==i);item_drop(p);}
    for(int32_t i=0;i<500;++i)list3_set(owned,i,item_new(-i),1,1);
    list3_clear(owned,1,1);assert(destroyed_objects==created_objects);list3_add(owned,item_new(42),1,1);ncr_drop_3(owned);
    NEW_LIST(4, erased);
    for(int32_t i=0;i<1000;++i){Iface f={item_new(i),erase_drop};list4_add(erased,f,1,1);}
    Iface extracted=list4_remove(erased,17,1,1);assert(((Item *)extracted.data)->n==17);
    ncr_drop_4(erased);assert(((Item *)extracted.data)->n==17);iface_drop(extracted);
    NEW_LIST(12, nested);
    for(int32_t i=0;i<1000;++i){NEW_LIST(0, row);list0_add(row,i,1,1);list12_add(nested,row,1,1);}
    ncr_obj_0 *row=list12_pop(nested,1,1);ncr_drop_12(nested);assert(list0_pop(row,1,1)==999);ncr_drop_0(row);
    ncr_obj_10 *text=ncr_allocate(sizeof(ncr_obj_10),1,1);text10_new(text,STR("é\0"),1,1);
    assert(text10_byte_length(text,1,1)==3);assert(text10_byte_at(text,2,1,1)==0);
    text10_append_int(text,INT32_MIN,1,1);text10_append_int(text,INT32_MAX,1,1);text10_append_boolean(text,true,1,1);text10_append_boolean(text,false,1,1);
    ncr_obj_10 *copy=text10_clone(text,1,1);assert(text10_equals(copy,text,1,1));text10_clear(text,1,1);assert(text10_is_empty(text,1,1));assert(!text10_equals(copy,text,1,1));
    text10_append_text(text,copy,1,1);assert(text10_equals(copy,text,1,1));text10_clear(text,1,1);assert(text10_equals_string(text,STR(""),1,1));
    for(int i=0;i<20000;++i)text10_append(text,STR("abc"),1,1);assert(text10_byte_length(text,1,1)==60000);ncr_drop_10(copy);
    NEW_LIST(11, texts);list11_add(texts,text,1,1);ncr_obj_10 *out=list11_pop(texts,1,1);ncr_drop_11(texts);assert(text10_byte_length(out,1,1)==60000);ncr_drop_10(out);
    assert(created_objects==destroyed_objects);assert(ncr_allocations==ncr_frees);
    puts("stdlib templates: all assertions passed");ncr_memory_report();return 0;
}
'''
NEGATIVE = {
    'negative_index':'NEW_LIST(0,xs);list0_add(xs,1,1,1);list0_get(xs,-1,7,9);',
    'past_end':'NEW_LIST(0,xs);list0_add(xs,1,1,1);list0_set(xs,1,0,7,9);',
    'remove_past_end':'NEW_LIST(0,xs);list0_remove(xs,0,7,9);',
    'pop_empty':'NEW_LIST(0,xs);list0_pop(xs,7,9);',
    'negative_capacity':'NEW_LIST(0,xs);list0_reserve(xs,-1,7,9);',
    'text_past_end':'ncr_obj_10 p={NULL,0,0};text10_byte_at(&p,0,7,9);',
    'text_size_overflow':'ncr_obj_10 p={NULL,0,0};ncr_text_append_10(&p,NULL,(size_t)INT32_MAX+1,7,9);',
    'list_size_overflow':'ncr_obj_0 p={NULL,INT32_MAX,INT32_MAX};list0_add(&p,0,7,9);',
}

def run(command, **kwargs):
    result=subprocess.run(command,capture_output=True,timeout=45,**kwargs)
    if result.returncode:
        raise AssertionError((command,result.returncode,result.stdout.decode(errors='replace'),result.stderr.decode(errors='replace')))
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cc',action='append',default=[])
    parser.add_argument('--report',type=Path)
    args=parser.parse_args()
    compilers=args.cc or [cc for cc in ('gcc','clang') if shutil.which(cc)]
    if not compilers: raise SystemExit('GCC/Clang missing')
    results=[]
    with tempfile.TemporaryDirectory(prefix='nacre-stdlib-') as temp:
        temp=Path(temp); source=temp/'test.c';exe=temp/'test'
        for cc in compilers:
            version=run([cc,'--version']).stdout.decode().splitlines()[0]
            for profile,extra in [('O2',[]),('sanitizers',['-O1','-g','-fsanitize=address,undefined','-fno-sanitize-recover=all','-fno-omit-frame-pointer','-fno-pie','-no-pie'])]:
                source.write_text(support()+POSITIVE)
                flags=['-std=c11','-O2','-Wall','-Wextra','-Wno-unused-function',*extra]
                run([cc,*flags,str(source),'-o',str(exe)])
                result=run([str(exe)],env=dict(os.environ,ASAN_OPTIONS='detect_leaks=1:halt_on_error=1'))
                assert result.stdout==b'stdlib templates: all assertions passed\n'
                assert b'live=0\n' in result.stderr
                results.append({'compiler':version,'profile':profile,'status':'passed','stderr':result.stderr.decode().strip()})
            for name,body in NEGATIVE.items():
                source.write_text(support()+'\n#define NEW_LIST(ID,NAME) ncr_obj_##ID *NAME=ncr_allocate(sizeof(ncr_obj_##ID),1,1);list##ID##_new(NAME,1,1)\nint main(void){'+body+'return 0;}\n')
                run([cc,'-std=c11','-O2',str(source),'-o',str(exe)])
                result=subprocess.run([str(exe)],capture_output=True,timeout=10)
                assert result.returncode==70,(name,result.returncode,result.stderr)
                assert b'[7:9]' in result.stderr
                results.append({'compiler':version,'case':name,'status':'correctly_aborted','exit_code':70})
    report={'scope':'exact shipped C templates instantiated by Python; NOT the Rust compiler or its actual C output',
            'status':'passed','randomized_list_operations':50000,'results':results}
    if args.report:args.report.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))
if __name__=='__main__':main()
