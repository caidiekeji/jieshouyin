import os, sys, json, base64, urllib.request, urllib.error, subprocess, time

TOKEN = os.environ.get('GH_TOKEN', '')
OWNER = 'caidiekeji'
REPO = 'jieshouyin'
ROOT = r'D:\shouyin'
API = 'https://api.github.com'

def api(method, path, body=None, retries=5):
    url = f'{API}{path}'
    data = json.dumps(body).encode('utf-8') if body is not None else None
    for attempt in range(retries):
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header('Authorization', f'Bearer {TOKEN}')
        req.add_header('Accept', 'application/vnd.github+json')
        req.add_header('X-GitHub-Api-Version', '2022-11-28')
        if data is not None:
            req.add_header('Content-Type', 'application/json')
        try:
            with urllib.request.urlopen(req, timeout=90) as r:
                return json.loads(r.read().decode('utf-8'))
        except urllib.error.HTTPError as e:
            detail = e.read().decode('utf-8')[:500]
            if e.code < 500 and e.code not in (409, 429):
                print(f'HTTP {e.code} {method} {path}: {detail}')
                raise
            print(f'  retry {attempt+1}/{retries} after HTTP {e.code}')
            time.sleep(2 * (attempt + 1))
        except Exception as e:
            print(f'  retry {attempt+1}/{retries} after {type(e).__name__}: {e}')
            time.sleep(2 * (attempt + 1))
    raise RuntimeError('all retries failed')

def get_sha(path):
    try:
        r = api('GET', f'/repos/{OWNER}/{REPO}/contents/{path}')
        return r.get('sha')
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return None
        raise

def upload_files(paths):
    # 获取主分支树
    main_ref = api('GET', f'/repos/{OWNER}/{REPO}/git/ref/heads/main')
    commit_sha = main_ref['object']['sha']
    commit = api('GET', f'/repos/{OWNER}/{REPO}/git/commits/{commit_sha}')
    base_tree = commit['tree']['sha']

    tree_items = []
    for p in paths:
        full = os.path.join(ROOT, p.replace('/', os.sep))
        with open(full, 'rb') as f:
            content = f.read()
        try:
            blob = api('POST', f'/repos/{OWNER}/{REPO}/git/blobs', {
                'content': base64.b64encode(content).decode('ascii'),
                'encoding': 'base64'
            })
        except Exception:
            continue
        tree_items.append({'path': p.replace('\\', '/'), 'mode': '100644', 'type': 'blob', 'sha': blob['sha']})
        print(f'  staged: {p}')

    created = api('POST', f'/repos/{OWNER}/{REPO}/git/trees', {'base_tree': base_tree, 'tree': tree_items})
    new_commit = api('POST', f'/repos/{OWNER}/{REPO}/git/commits', {
        'message': sys.argv[1] if len(sys.argv) > 1 else 'chore: update files',
        'tree': created['sha'],
        'parents': [commit_sha]
    })
    api('PATCH', f'/repos/{OWNER}/{REPO}/git/refs/heads/main', {'sha': new_commit['sha'], 'force': True})
    print(f'DONE commit={new_commit["sha"]}')

if __name__ == '__main__':
    mode = sys.argv[2] if len(sys.argv) > 2 else 'changed'
    if mode == 'all':
        out = subprocess.check_output(['git', '-c', 'core.quotepath=false', 'ls-tree', '-r', '--name-only', 'HEAD'], cwd=ROOT).decode('utf-8')
        paths = [f for f in out.strip().split('\n') if f]
    else:
        out = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT).decode('utf-8')
        paths = []
        for line in out.strip().split('\n'):
            if not line:
                continue
            p = line[3:]
            if ' -> ' in p:
                p = p.split(' -> ')[-1]
            paths.append(p)
    print(f'Uploading {len(paths)} files')
    upload_files(paths)
