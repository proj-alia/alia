#!/usr/bin/env python3
"""Offline pinned V4.1-1017 text-only framing/ByteLevel bound verification.

No credentials, provider calls, or downloaded runtime tokenizer are required.
Hosted gateway compatibility still needs a separately accounted live call.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import random
from tokenizers import Tokenizer

REVISION = '08c8657f02291e11b6f6b63e0e16014f8e748518'
FILES = {
    'tokenizer.json': 'c90dfa01249db1be4245780a052ede752e1361c612ac6d08e2bdada7d599476b',
    'tokenizer_config.json': '6ac8c8dc065ed118161d02dd532749ae3f52c243deac27872134fae2f50d8547',
    'encoding/encoding.py': '13231c592e958fb39e2af6001dae69d90e7a36b265702b9b390236fb387599e2',
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--cache', type=Path, default=Path('/tmp/alia-deepseek-budget'))
    args = parser.parse_args()
    blobs = {name: (args.cache / name).read_bytes() for name in FILES}
    for name, digest in FILES.items():
        assert hashlib.sha256(blobs[name]).hexdigest() == digest, name
    config = json.loads(blobs['tokenizer.json'])
    assert config['normalizer'] == {'type': 'Sequence', 'normalizers': []}
    assert config['model']['type'] == 'BPE'
    assert config['model']['continuing_subword_prefix'] is None
    assert config['model']['end_of_word_suffix'] is None
    assert config['padding'] is None and config['truncation'] is None
    pretokens = config['pre_tokenizer']['pretokenizers']
    assert len(pretokens) == 4
    assert all(p['type'] == 'Split' and p['behavior'] == 'Isolated' and not p['invert'] for p in pretokens[:3])
    assert pretokens[3] == {'type': 'ByteLevel', 'add_prefix_space': False, 'trim_offsets': True, 'use_regex': False}
    assert config['post_processor']['type'] == 'ByteLevel'
    alphabet = list(range(33, 127)) + list(range(161, 173)) + list(range(174, 256))
    chars = alphabet.copy()
    for byte in range(256):
        if byte not in alphabet:
            chars.append(256 + len(alphabet) - 188)
            alphabet.append(byte)
    assert len(chars) == 256 and len(set(chars)) == 256
    assert all(chr(c) in config['model']['vocab'] for c in chars)
    assert all(t['content'] for t in config['added_tokens'])
    tokenizer = Tokenizer.from_str(blobs['tokenizer.json'].decode())
    # Load only the hash-pinned, self-contained upstream encoder reviewed above.
    spec = importlib.util.spec_from_file_location('deepseek_encoder', args.cache / 'encoding/encoding.py')
    encoder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(encoder)
    cases = ['', 'hello', '中文观察🙂', 'é'*256, 'e\u0301'*256, ''.join(map(chr, range(128))), '甲'*10000, 'a'*240000, '<｜System｜><think>']
    rng = random.Random(41)
    cases += [''.join(chr(rng.choice([65, 0x4e2d, 0x1f642, 0x301, 0, 0x10ffff, 0x200d])) for _ in range(200)) for _ in range(24)]
    count = 0
    max_frame = 0
    for mode in ['chat', 'thinking']:
        for effort in ['low', 'high', 'max']:
            for response_format in [None, {'type': 'json_object'}]:
                empty = [{'role': 'system', 'content': ''}, {'role': 'user', 'content': ''}]
                if response_format:
                    empty[0]['response_format'] = response_format
                frame = encoder.encode_messages(empty, thinking_mode=mode, reasoning_effort=effort)
                frame_bytes = len(frame.encode())
                max_frame = max(max_frame, frame_bytes)
                assert frame_bytes <= 4096
                for text in cases:
                    messages = [{'role': 'system', 'content': '系统规范 / JSON schema'}, {'role': 'user', 'content': text}]
                    body = {'model': 'deepseek-v4.1-flash', 'messages': messages, 'response_format': {'type': 'json_object'}, 'reasoning_effort': effort, 'max_tokens': 4096, 'stream': False}
                    wire_bytes = len(json.dumps(body, ensure_ascii=False, separators=(',', ':')).encode())
                    if response_format:
                        messages[0]['response_format'] = response_format
                    prompt = encoder.encode_messages(messages, thinking_mode=mode, reasoning_effort=effort)
                    assert len(prompt.encode()) == frame_bytes + sum(len(m['content'].encode()) for m in messages)
                    encoded = tokenizer.encode(prompt, add_special_tokens=True)
                    assert encoded.ids == tokenizer.encode(prompt, add_special_tokens=False).ids
                    assert len(encoded.ids) <= len(prompt.encode()) <= wire_bytes + 4096
                    count += 1
    print(json.dumps({'status': 'passed', 'revision': REVISION, 'hashes': FILES, 'framed_cases': count, 'max_published_framing_bytes': max_frame, 'runtime_framing_reserve': 4096, 'scope': 'two text messages only; hosted compatibility requires live usage evidence'}))

if __name__ == '__main__':
    main()
