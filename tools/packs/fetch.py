"""Download the recordings listed in manifest.json.

    python fetch.py manifest.json <download dir>

Files land in <download dir>/<repo>/<path>. Anything already there with the right
size is kept, so the script can be re-run.
"""
import json
import os
import sys
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor

MANIFEST, DEST = sys.argv[1], sys.argv[2]
BASE = "https://raw.githubusercontent.com/sfzinstruments/"

wanted = {}
for pack in json.load(open(MANIFEST, encoding="utf-8"))["packs"]:
    for sound in pack["sounds"]:
        for f in sound["sources"]:
            wanted[(f["repo"], f["branch"], f["path"])] = f["size"]


def fetch(item):
    (repo, branch, path), size = item
    target = os.path.join(DEST, repo, *path.split("/"))
    if os.path.isfile(target) and os.path.getsize(target) == size:
        return 0
    os.makedirs(os.path.dirname(target), exist_ok=True)
    url = BASE + repo + "/" + branch + "/" + urllib.parse.quote(path)
    request = urllib.request.Request(url, headers={"User-Agent": "sample-pack-fetch"})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    if len(data) != size:
        raise RuntimeError("%s: got %d bytes, expected %d" % (path, len(data), size))
    with open(target + ".part", "wb") as out:
        out.write(data)
    os.replace(target + ".part", target)
    return len(data)


with ThreadPoolExecutor(8) as pool:
    done = list(pool.map(fetch, sorted(wanted.items())))
print("%d files wanted, %d downloaded now, %.1f MB" % (len(wanted), sum(1 for d in done if d), sum(done) / 1048576.0))
