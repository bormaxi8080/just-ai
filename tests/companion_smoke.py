"""Portable CLI/MCP contract regressions against a local, credential-free provider."""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bin-dir', required=True)
    binaries = Path(parser.parse_args().bin_dir).resolve()
    suffix = '.exe' if os.name == 'nt' else ''
    captured = []
    replies = [
        {'summary': 'review', 'recommendations': []},
        {'summary': 'review', 'template': {'name': 'example', 'description': 'test', 'category': 'test', 'parameters': [], 'body': ['@echo stored-marker']}},
        {'summary': 'review', 'rationale': [], 'recipe': {'name': 'blocked', 'doc': None, 'parameters': [], 'dependencies': [], 'body': ['rm -rf /']}},
    ]

    class Provider(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_POST(self):
            captured.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
            payload = json.dumps({'done': True, 'message': {'content': json.dumps(replies.pop(0))}}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    server = http.server.HTTPServer(('127.0.0.1', 0), Provider)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='just-ai-contracts-') as directory:
            root = Path(directory)
            (root / 'justfile').write_text('hello:\n  @echo API_KEY=synthetic-smoke-secret\n', encoding='utf8')
            (root / 'just-ai.toml').write_text(f'[ai]\nprovider = "ollama"\nmodel = "mock"\nbase_url = "http://127.0.0.1:{server.server_port}"\n[history]\nbackend = "jsonl"\n', encoding='utf8')
            env = {key: value for key, value in os.environ.items() if not key.startswith('JUST_AI_')}
            env['PATH'] = str(binaries) + os.pathsep + env['PATH']
            env['JUST_AI_DATA_DIR'] = str(root / 'data')
            result = subprocess.run([str(binaries / ('just-ai' + suffix)), 'suggest'], cwd=root, env=env, capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, result.stderr

            def call(identifier, name, arguments):
                return {'jsonrpc': '2.0', 'id': identifier, 'method': 'tools/call', 'params': {'name': name, 'arguments': arguments}}

            messages = [
                {'jsonrpc': '2.0', 'id': 1, 'method': 'initialize', 'params': {'protocolVersion': '2025-06-18', 'capabilities': {}, 'clientInfo': {'name': 'contract-smoke', 'version': '1'}}},
                call(2, 'create_template', {'request': 'safe template', 'write': True}),
                call(3, 'instantiate_template', {'template': 'example', 'values': {}, 'write': True}),
                call(4, 'run_recipe', {'recipe': 'example'}),
                call(5, 'get_history', {'recipe': 'example'}),
                call(6, 'add_recipe', {'request': 'unsafe proposal fixture', 'write': True}),
                call(7, 'migrate_modularize', {'write': 'true'}),
            ]
            result = subprocess.run([str(binaries / ('just-ai-mcp' + suffix))], cwd=root, env=env, input=''.join(json.dumps(message) + '\n' for message in messages), capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, result.stderr
            responses = {response['id']: response for response in map(json.loads, result.stdout.splitlines())}
            assert responses[2]['result']['structuredContent']['written'] is True
            assert responses[3]['result']['structuredContent']['written'] is True
            assert responses[4]['result']['structuredContent']['success'] is True
            assert len(responses[5]['result']['structuredContent']['records']) == 1
            assert 'blocked' in json.dumps(responses[6]).lower()
            assert responses[6].get('error') or responses[6].get('result', {}).get('isError')
            assert responses[7].get('error') or responses[7].get('result', {}).get('isError')
            source = (root / 'justfile').read_text(encoding='utf8')
            assert 'stored-marker' in source and 'rm -rf /' not in source
            assert (root / '.just-ai' / 'templates' / 'example.json').is_file()
            assert not replies, 'Expected provider calls did not occur'
            assert 'synthetic-smoke-secret' not in json.dumps(captured), 'Secret reached provider transport'
            print('CLI/MCP smoke: TOML provider, redaction, stored templates, history and blocked writes passed')
    finally:
        server.shutdown()
        worker.join(timeout=5)
        server.server_close()


if __name__ == '__main__':
    main()
