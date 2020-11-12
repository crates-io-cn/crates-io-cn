import os
import json
from multiprocessing import Pool
from queue import Queue
import threading
from subprocess import Popen, DEVNULL

with open('checkpoint.txt', 'r') as reader:
    history = set([line.strip() for line in reader.readlines()])

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

def success_comsumer():
    while True:
        key = success_queue.get()
        print(f'{key}: success')
        checkpoint.write(f'{key}\n')
        history.add(key)
        checkpoint.flush()

def failure_comsumer():
    while True:
        key = failure_queue.get()
        print(f'{key}: fail')

def main():
    workers = [threading._start_new_thread(download, tuple()) for i in range(32)]
    workers.append(threading._start_new_thread(success_comsumer, tuple()))
    workers.append(threading._start_new_thread(failure_comsumer, tuple()))
    for root, _, crates in os.walk('/mirror_nfs/crates.io-index'):
        for crate in crates:
            if crate.endswith('json'):
                continue
            checkouts = [json.loads(line) for line in open(os.path.join(root, crate)).readlines()]
            keys = [f'{checkout["name"]}/{checkout["vers"]}' for checkout in checkouts]
            for key in keys:
                if key in history:
                    continue
                task_pool.put(key)
    task_pool.join()
            

if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        checkpoint.close()