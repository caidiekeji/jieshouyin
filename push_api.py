import os, sys, json, base64, urllib.request, urllib.error, subprocess

TOKEN = os.environ.get('GH_TOKEN', '')
OWNER = 'caidiekeji'
REPO = 'jieshouyin'
ROOT = r'D:\shouyin'
API = 'https://api.github.com'

import time

def api(method, path, body=None, retries=5):
    url = f'{API}{path}'
    data = json.dumps(body).encode('utf-8') if body is not None else None
    last_err = None
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
            # 4xx 除 409/429 外不重试
            if e.code < 500 and e.code not in (409, 429):
                print(f'HTTP {e.code} {method} {path}: {detail}')
                raise
            last_err = e
            print(f'  retry {attempt+1}/{retries} after HTTP {e.code}')
            time.sleep(2 * (attempt + 1))
        except Exception as e:
            last_err = e
            print(f'  retry {attempt+1}/{retries} after {type(e).__name__}: {e}')
            time.sleep(2 * (attempt + 1))
    raise last_err

# 读取 HEAD commit 中的文件列表（本地已 commit）
out = subprocess.check_output(['git', '-c', 'core.quotepath=false', 'ls-tree', '-r', '--name-only', 'HEAD'], cwd=ROOT).decode('utf-8')
files = [f for f in out.strip().split('\n') if f]
print(f'Files to upload: {len(files)}')

# 空仓库的 git blobs API 会返回 409，先通过 Contents API 创建初始文件建立分支
try:
    api('GET', f'/repos/{OWNER}/{REPO}/git/ref/heads/main')
    print('main ref exists')
except urllib.error.HTTPError:
    print('Initializing main branch via Contents API...')
    api('PUT', f'/repos/{OWNER}/{REPO}/contents/.gitkeep', {
        'message': 'init: bootstrap repository',
        'content': base64.b64encode(b'').decode('ascii'),
        'branch': 'main'
    })

# 创建 blobs
tree = []
for f in files:
    full = os.path.join(ROOT, f.replace('/', os.sep))
    with open(full, 'rb') as fh:
        content = fh.read()
    blob = api('POST', f'/repos/{OWNER}/{REPO}/git/blobs', {
        'content': base64.b64encode(content).decode('ascii'),
        'encoding': 'base64'
    })
    tree.append({
        'path': f.replace('\\', '/'),
        'mode': '100644',
        'type': 'blob',
        'sha': blob['sha']
    })
    print(f'  blob: {f}')

# 创建 tree
created_tree = api('POST', f'/repos/{OWNER}/{REPO}/git/trees', {
    'tree': tree
})

# 创建 commit（以当前 main 为父提交）
main_ref = api('GET', f'/repos/{OWNER}/{REPO}/git/ref/heads/main')
parent_sha = main_ref['object']['sha']
commit = api('POST', f'/repos/{OWNER}/{REPO}/git/commits', {
    'message': 'feat: Checkout-R 核心领域库 + Tauri2 应用 + Vue 前端 + CI',
    'tree': created_tree['sha'],
    'parents': [parent_sha]
})
print(f'Commit: {commit["sha"]}')

# 更新 main ref
ref = api('PATCH', f'/repos/{OWNER}/{REPO}/git/refs/heads/main', {
    'sha': commit['sha'],
    'force': True
})
print(f'Ref updated: {ref["object"]["sha"]}')
print('DONE')
