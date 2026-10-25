#!/bin/bash
# cru 定时任务模拟：每 60 秒执行一遍 /tmp/lab/cron 里每分钟一次（* 或 */1）的任务；
# 每周一次这类定点任务不在沙盒里自动跑（场景需要时直接调用对应动作）
while sleep 60; do
	[ -f /tmp/lab/cron ] || continue
	[ -f /tmp/lab/crond.paused ] && continue
	cut -f2- /tmp/lab/cron | while IFS= read -r line; do
		case "${line%% *}" in '*' | '*/1') ;; *) continue ;; esac
		cmd=$(echo "$line" | cut -d' ' -f6-)
		echo "$(date +%T) run: $cmd"
		bash -c "$cmd" &
	done
done
