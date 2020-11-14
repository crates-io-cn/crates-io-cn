import os
import json
from hashlib import sha256
from mmap import mmap, PROT_READ
from _thread import start_new_thread
from multiprocessing import Manager, Process, JoinableQueue

from tqdm import tqdm

manager = Manager()

index = manager.dict()
task_pool = JoinableQueue(200)
success_queue = JoinableQueue()
failure_queue = JoinableQueue()

def checker():
    while True:
        crate, ver = task_pool.get()
        checksum = sha256sum(f'/mirror_nfs/crates/{crate}/{ver}')
        if checksum == index[(crate, ver)]:
            success_queue.put((crate, ver))
        else:
            failure_queue.put((crate, ver, checksum, index[(crate, ver)]))
        index.pop((crate, ver))
        task_pool.task_done()

def success_comsumer():
    while True:
        crate, ver = success_queue.get()
        #print(f'{crate}-{ver}: pass')
        success_queue.task_done()

def failure_comsumer():
    while True:
        crate, ver, checksum, expected = failure_queue.get()
        print(f'{crate}-{ver}: fail, {checksum} != {expected}')
        failure_queue.task_done()

def main():
    workers = [Process(target=checker, args=tuple()) for i in range(32)]
    for worker in workers:
        worker.start()
    start_new_thread(success_comsumer, tuple())
    start_new_thread(failure_comsumer, tuple())
    for crate in tqdm(os.listdir('/mirror_nfs/crates'), ncols=80):
        checkouts = [json.loads(line) for line in open(os.path.join('/mirror_nfs/crates.io-index', to_name(crate))).readlines()]
        for checkout in tqdm(checkouts, desc=crate, leave=False, ncols=80):
            index[(crate, checkout["vers"])] = checkout["cksum"]
        for ver in os.listdir(os.path.join('/mirror_nfs/crates', crate)):
            task_pool.put((crate, ver))
    task_pool.close()
    task_pool.join()
    success_queue.close()
    failure_queue.close()
    for worker in workers:
        worker.terminate()
    success_queue.join()
    failure_queue.join()


def sha256sum(filename) -> str:
    h  = sha256()
    with open(filename, 'rb') as f:
        with mmap(f.fileno(), 0, prot=PROT_READ) as mm:
            h.update(mm)
    return h.hexdigest()

"""
    Packages with 1 character names are placed in a directory named 1.
    Packages with 2 character names are placed in a directory named 2.
    Packages with 3 character names are placed in the directory 3/{first-character} where {first-character} is the first character of the package name.
    All other packages are stored in directories named {first-two}/{second-two} where the top directory is the first two characters of the package name, and the next subdirectory is the third and fourth characters of the package name. For example, cargo would be stored in a file named ca/rg/cargo.
"""
def to_name(crate) -> str:
    crate = crate.lower()
    if len(crate) == 1:
        return f'1/{crate}'
    elif len(crate) == 2:
        return f'2/{crate}'
    elif len(crate) == 3:
        return f'3/{crate[0]}/{crate}'
    else:
        return f'{crate[:2]}/{crate[2:4]}/{crate}'


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        pass