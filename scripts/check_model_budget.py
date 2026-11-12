#!/usr/bin/env python3
"""Verify pinned public tokenizer/template against our text-only byte bound.
No credentials, inference or private inputs. Downloads are cached outside repo.
"""
import argparse
import hashlib
import json
from pathlib import Path
import random
import subprocess
import urllib.request
from jinja2.sandbox import SandboxedEnvironment
from tokenizers import Tokenizer

REVISION = '46a050bec13002ad29ee795c16e61cf4fa3c4a9c'
FILES = {
    'tokenizer.json': '19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d',
    'tokenizer_config.json': '98b1271574f41abf89427ae2dda030d94dc9478f0edc5a8bd240db213c6fd5fc',
    'chat_template.jinja': '2614a3c896d53e265f564c23701173c0049f985dacfbae79fc4bcd9f5ba083f4',
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cache', type=Path, default=Path('/tmp/alia-budget-artifacts'))
    parser.add_argument('--rust-binary', type=Path, help='Compare the Rust runtime counter against the pinned Python/Jinja implementation')
    args = parser.parse_args()
    args.cache.mkdir(parents=True, exist_ok=True)
    blobs = {}
    for name, sha in FILES.items():
        path = args.cache / name
        if not path.exists():
            url = f'https://huggingface.co/zai-org/GLM-5.3-Flash/resolve/{REVISION}/{name}'
            request = urllib.request.Request(url, headers={'User-Agent': 'Alia/0.1'})
            with urllib.request.urlopen(request, timeout=30) as response:
                data = response.read(32 * 1024 * 1024 + 1)
            assert len(data) <= 32 * 1024 * 1024
            assert hashlib.sha256(data).hexdigest() == sha
            path.write_bytes(data)
        blobs[name] = path.read_bytes()
        assert hashlib.sha256(blobs[name]).hexdigest() == sha, 'Changed model artifact'
    config = json.loads(blobs['tokenizer.json'])
    assert config['normalizer'] is None
    assert config['model']['type'] == 'BPE'
    assert config['model']['continuing_subword_prefix'] is None
    assert config['model']['end_of_word_suffix'] is None
    assert config['pre_tokenizer']['pretokenizers'][1] == {
        'type': 'ByteLevel', 'add_prefix_space': False, 'trim_offsets': True, 'use_regex': False}
    assert config['post_processor']['type'] == 'ByteLevel'
    assert all(len(t['content'].encode()) >= 1 for t in config['added_tokens'])
    # Every byte has a one-token representation under the GPT-2 ByteLevel map.
    bs = list(range(33, 127)) + list(range(161, 173)) + list(range(174, 256))
    cs = bs.copy()
    extra = 0
    for byte in range(256):
        if byte not in bs:
            bs.append(byte)
            cs.append(256 + extra)
            extra += 1
    assert all(chr(c) in config['model']['vocab'] for c in cs)
    tokenizer = Tokenizer.from_str(blobs['tokenizer.json'].decode())
    env = SandboxedEnvironment(extensions=['jinja2.ext.loopcontrols'])
    template = env.from_string(blobs['chat_template.jinja'].decode())
    assert len(blobs['chat_template.jinja']) == 10830
    # Exhaustive branch analysis for the only admitted request shape. With
    # tools=None and exactly system/user string content, visible_text is identity;
    # the assistant/tool/multimodal branches cannot execute. Hash pins this code.
    def framing(effort):
        return '[gMASK]<sop><|system|>Reasoning Effort: '+effort.capitalize()+'<|system|><|user|><|assistant|><think>'
    frame_bound=max(len(framing(e).encode()) for e in ['low','high','max'])
    assert frame_bound==82
    rng = random.Random(11)
    cases = ['', 'hello', '中文观察🙂', '\\n\n\t"\\', '<|system|>[gMASK]<think>',
             'é' * 256, 'e\u0301' * 256, ''.join(map(chr, range(128))), '甲' * 10000]
    cases.extend(['a' * 240000, ('e\u0301' * 20000), ('<|system|>\\\"' * 4000)])
    for _ in range(24):
        cases.append(''.join(chr(rng.choice([0x41, 0x4e2d, 0x1f642, 0x301, 0, 0x10ffff, 0x200d])) for _ in range(200)))
    runtime_cases = []
    runtime_counts = []
    for content in cases:
        encoded = tokenizer.encode(content, add_special_tokens=False)
        assert len(encoded.ids) <= len(content.encode())
        messages = [{'role': 'system', 'content': '系统规范 / JSON schema'}, {'role': 'user', 'content': content}]
        for effort in ['low', 'high', 'max']:
            prompt = template.render(messages=messages, tools=None, add_generation_prompt=True, reasoning_effort=effort)
            expected='[gMASK]<sop><|system|>Reasoning Effort: '+effort.capitalize()+'<|system|>'+messages[0]['content']+'<|user|>'+content+'<|assistant|><think>'
            assert prompt==expected, 'Unaccounted framing or content transformation'
            assert len(prompt.encode())<=sum(len(m['content'].encode()) for m in messages)+frame_bound
            body_bytes = len(json.dumps({'messages': messages}, ensure_ascii=False, separators=(',', ':')).encode())
            assert len(tokenizer.encode(prompt, add_special_tokens=True).ids) <= body_bytes + frame_bound
            if effort=='low':
                assert tokenizer.encode(prompt, add_special_tokens=True).ids==tokenizer.encode(prompt, add_special_tokens=False).ids
                runtime_cases.append((messages[0]['content'], content))
                runtime_counts.append(len(tokenizer.encode(prompt, add_special_tokens=False).ids))
    if args.rust_binary:
        result=subprocess.run([str(args.rust_binary.resolve()),str(args.cache/'tokenizer.json')],input=json.dumps(runtime_cases,ensure_ascii=False),capture_output=True,text=True,check=True)
        assert json.loads(result.stdout)==runtime_counts, 'Rust counter diverges from pinned tokenizer/Jinja'

    print(json.dumps({'status':'passed', 'revision':REVISION, 'hashes':FILES,
                      'text_cases':len(cases), 'framed_cases':3*len(cases), 'framing_reserve':frame_bound,'rust_exact_cases':len(runtime_cases) if args.rust_binary else 0,
                      'scope':'published tokenizer/template only; hosted compatibility requires separate live evidence'}))

if __name__ == '__main__':
    main()
