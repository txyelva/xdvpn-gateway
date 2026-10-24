#!/bin/bash
# cru 定时任务模拟：每 60 秒执行一遍 /tmp/lab/cron 里的 */1 任务
while sleep 60; do
	[ -f /tmp/lab/cron ] || continue
	[ -f /tmp/lab/crond.paused ] && continue
	cut -f2- /tmp/lab/cron | while IFS= read -r line; do
		cmd=$(echo "$line" | cut -d' ' -f6-)
		echo "$(date +%T) run: $cmd"
		bash -c "$cmd" &
	done
done
