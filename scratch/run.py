import os
import sys
import json
from multiprocessing import Pool
from queue import Queue
import threading
from subprocess import Popen, DEVNULL
from tqdm import tqdm

with open('checkpoint.txt', 'r') as reader:
    history = set([line.strip() for line in tqdm(reader.readlines(), leave=False, ncols=80)])

checkpoint = open('checkpoint.txt', 'a')

task_pool = Queue(64)
success_queue = Queue()
failure_queue = Queue()

def download():
    while True:
        key = task_pool.get()
        ret = Popen(['/usr/bin/curl', '-f', f'https://crates.exp.lightsing.me/api/v1/crates/{key}/download'], stdout=DEVNULL, stderr=DEVNULL).wait()
        if ret == 0:
            success_queue.put(key)
        else:
            failure_queue.put((key, ret))
        task_pool.task_done()

def success_comsumer():
    while True:
        key = success_queue.get()
        #print(f'{key}: success')
        checkpoint.write(f'{key}\n')
        history.add(key)
        checkpoint.flush()
        success_queue.task_done()

def failure_comsumer():
    while True:
        key = failure_queue.get()
        tqdm.write(f'{key}: fail', file=sys.stderr)
        failure_queue.task_done()

def main():
    workers = [threading._start_new_thread(download, tuple()) for i in range(32)]
    workers.append(threading._start_new_thread(success_comsumer, tuple()))
    workers.append(threading._start_new_thread(failure_comsumer, tuple()))
    prefix = '/mirror_nfs/crates.io-index'
    for root, _, crates in tqdm(os.walk(prefix), ncols=80, total=14690):
        if '.git' in root:
            continue
        for crate in tqdm(crates, desc=root[len(prefix):], leave=False, ncols=80):
            if crate.endswith('json'):
                continue
            try:
                checkouts = [json.loads(line) for line in open(os.path.join(root, crate)).readlines()]
                keys = [f'{checkout["name"]}/{checkout["vers"]}' for checkout in checkouts if not checkout["yanked"]]
                #keys = [key for key in keys if os.path.isfile(f'/mirror_nfs/crates/{key}')]
                keys = [key for key in keys if not key in history]
                for key in keys:
                    task_pool.put(key)
            except Exception as e:
                tqdm.write(os.path.join(root, crate), file=sys.stderr)
                tqdm.write(e, file=sys.stderr)
    task_pool.join()
    success_queue.join()
    failure_queue.join()
            

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        checkpoint.close()
        success_queue.join()
        failure_queue.join()