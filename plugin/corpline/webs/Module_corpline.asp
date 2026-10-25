<!DOCTYPE html>
<html>
<head>
<meta http-equiv="X-UA-Compatible" content="IE=Edge">
<meta http-equiv="Content-Type" content="text/html; charset=UTF-8">
<meta HTTP-EQUIV="Pragma" CONTENT="no-cache">
<meta HTTP-EQUIV="Expires" CONTENT="-1">
<link rel="shortcut icon" href="images/favicon.png">
<link rel="icon" href="images/favicon.png">
<title>公司专线</title>
<link rel="stylesheet" type="text/css" href="index_style.css">
<link rel="stylesheet" type="text/css" href="form_style.css">
<link rel="stylesheet" type="text/css" href="/res/softcenter.css">
<script type="text/javascript" src="/js/jquery.js"></script>
<script type="text/javascript" src="/state.js"></script>
<script type="text/javascript" src="/general.js"></script>
<script type="text/javascript" src="/popup.js"></script>
<script type="text/javascript" src="/help.js"></script>
<script type="text/javascript" src="/res/softcenter.js"></script>
<style>
/* 公司专线：全部样式限定在 .cl 内，不影响华硕后台外框 */
.cl{
  --surface:#414E52; --line:#5C696D; --input:#2F3A3E; --input-line:#6A7A7F;
  --t1:#FFFFFF; --t2:#D0D8DB; --t3:#B4BFC2;
  --blue:#248DFF; --blue-hover:#3A99FF; --blue-press:#1C7BE3;
  --ok:#46D17F; --warn:#F5A623; --err:#FF6B60; --neutral:#D5DEE1; --idle:#8E9A9E;
  --ease:cubic-bezier(.2,.8,.2,1); --d-fast:150ms; --d-mid:200ms;
  box-sizing:border-box; width:760px; background:#4D595D; border-radius:0 0 3px 3px; padding:20px 24px 24px;
  color:var(--t1); text-align:left; font-size:13px; line-height:20px; font-weight:400;
  font-family:-apple-system,BlinkMacSystemFont,"PingFang SC","Hiragino Sans GB","Microsoft YaHei","Noto Sans CJK SC","Source Han Sans SC","Helvetica Neue",Arial,sans-serif;
  -webkit-font-smoothing:antialiased; text-shadow:none;
}
.cl *,.cl *::before,.cl *::after{box-sizing:border-box}
.cl h1,.cl h2,.cl p,.cl ul,.cl li{margin:0;padding:0;font:inherit;color:inherit;list-style:none;text-shadow:none}
.cl button,.cl input,.cl select{font-family:inherit;margin:0;text-shadow:none;box-shadow:none}
.cl a{color:var(--t2);text-decoration:none}
.cl a:hover{color:var(--t1)}
.cl .cl-num{font-variant-numeric:tabular-nums}
.cl [hidden]{display:none!important}

/* 顶部 */
.cl .cl-crumb{display:flex;justify-content:space-between;font-size:12px;line-height:18px;color:var(--t3)}
.cl .cl-head{display:flex;align-items:flex-start;gap:16px;margin-top:16px}
.cl .cl-head .cl-tt{flex:1;min-width:0}
.cl .cl-h1{font-size:20px;line-height:26px;font-weight:600}
.cl .cl-sub{font-size:13px;line-height:20px;color:var(--t2);margin-top:4px}
.cl .cl-sw{display:flex;align-items:center;gap:10px;padding-top:3px;font-size:13px;line-height:20px;color:var(--t2);white-space:nowrap}
.cl .cl-flash{font-size:12px;line-height:18px;color:var(--err);max-height:0;opacity:0;overflow:hidden;transition:opacity var(--d-fast) var(--ease),max-height var(--d-mid) var(--ease),margin var(--d-mid) var(--ease)}
.cl .cl-flash.on{max-height:40px;opacity:1;margin-top:8px}

/* 开关 */
.cl .cl-tg{position:relative;width:40px;height:24px;border-radius:9999px;background:#6A777B;border:0;padding:0;cursor:pointer;flex:none;transition:background var(--d-fast) var(--ease)}
.cl .cl-tg::after{content:"";position:absolute;top:2px;left:2px;width:20px;height:20px;border-radius:9999px;background:#fff;transition:transform var(--d-fast) var(--ease)}
.cl .cl-tg::before{content:"";position:absolute;top:-4px;right:-4px;bottom:-4px;left:-4px}
.cl .cl-tg[aria-checked="true"]{background:var(--blue)}
.cl .cl-tg[aria-checked="true"]::after{transform:translateX(16px)}
.cl .cl-tg:disabled{opacity:.45;cursor:not-allowed}
.cl .cl-tg.busy{cursor:progress}
.cl .cl-tg:focus,.cl .cl-btn:focus,.cl .cl-row-h:focus,.cl .cl-lnk:focus,.cl .cl-dname:focus,.cl .cl-seg button:focus{outline:2px solid var(--blue);outline-offset:2px}
/* 鼠标点击不显示焦点框；不认识 :focus-visible 的旧浏览器（Safari 15.4 以前）会整条忽略这一规则，保留上面的焦点框 */
.cl .cl-tg:focus:not(:focus-visible),.cl .cl-btn:focus:not(:focus-visible),.cl .cl-row-h:focus:not(:focus-visible),.cl .cl-lnk:focus:not(:focus-visible),.cl .cl-dname:focus:not(:focus-visible),.cl .cl-seg button:focus:not(:focus-visible){outline:none}

/* 状态卡 */
.cl .cl-card{background:var(--surface);border:1px solid var(--line);border-radius:8px}
.cl .cl-status{margin-top:20px;padding:20px}
.cl .cl-st-body{transition:opacity var(--d-fast) var(--ease)}
.cl .cl-st-body.fade{opacity:0}
.cl .cl-st-top{display:flex;align-items:center;gap:10px;min-height:32px}
.cl .cl-dot{display:inline-block;width:10px;height:10px;border-radius:9999px;flex:none;background:var(--neutral)}
.cl .cl-dot.ok{background:var(--ok)} .cl .cl-dot.warn{background:var(--warn)} .cl .cl-dot.err{background:var(--err)} .cl .cl-dot.idle{background:var(--idle)}
.cl .cl-dot.pulse{animation:clPulse 1.6s var(--ease) 2}
@keyframes clPulse{50%{opacity:.35}}
.cl .cl-st-title{font-size:18px;line-height:24px;font-weight:600;flex:1;min-width:0}
.cl .cl-st-desc{font-size:13px;line-height:20px;color:var(--t2);margin:8px 0 0 20px;max-width:560px;word-break:keep-all;overflow-wrap:anywhere}
.cl .cl-metrics{display:grid;grid-template-columns:repeat(4,1fr);margin-top:20px;padding-top:16px;border-top:1px solid var(--line)}
.cl .cl-metrics:empty{display:none}
.cl .cl-mt{padding-right:12px}
.cl .cl-mt + .cl-mt{padding-left:16px;border-left:1px solid var(--line)}
.cl .cl-mt .l{font-size:12px;line-height:18px;color:var(--t3)}
.cl .cl-mt .v{font-size:16px;line-height:22px;font-weight:500;margin-top:4px;white-space:nowrap}
.cl .cl-mt .v.wrap{white-space:normal;overflow-wrap:anywhere}
.cl .cl-mt .v .gap{display:inline-block;width:8px}
.cl .cl-mt .s{font-size:12px;line-height:18px;color:var(--t3);margin-top:2px}

/* 按钮 */
.cl .cl-btn{height:32px;padding:0 14px;border-radius:6px;font-size:13px;line-height:30px;font-weight:500;cursor:pointer;border:1px solid transparent;white-space:nowrap;flex:none;transition:background var(--d-fast) var(--ease),border-color var(--d-fast) var(--ease),opacity var(--d-fast) var(--ease),transform 80ms var(--ease)}
.cl .cl-btn:active{transform:scale(.98)}
.cl .cl-btn.pri{background:var(--blue);color:#fff}
.cl .cl-btn.pri:hover{background:var(--blue-hover)}
.cl .cl-btn.pri:active{background:var(--blue-press)}
.cl .cl-btn.sec{background:transparent;border-color:#7D8B90;color:var(--t1)}
.cl .cl-btn.sec:hover{background:rgba(255,255,255,.06)}
.cl .cl-btn:disabled{opacity:.55;cursor:default;transform:none}
.cl .cl-btn.sec:disabled:hover{background:transparent}
.cl .cl-btn.pri:disabled:hover{background:var(--blue)}

/* 分节 */
.cl .cl-sec-h{display:flex;align-items:baseline;justify-content:space-between;margin:28px 0 10px}
.cl .cl-sec-h h2{font-size:14px;line-height:20px;font-weight:600}
.cl .cl-lnk{font-size:12px;line-height:18px;color:var(--t2);background:none;border:0;padding:7px 6px;margin:-7px -6px;cursor:pointer;border-radius:4px;white-space:nowrap;transition:color var(--d-fast) var(--ease)}
.cl .cl-lnk:hover{color:var(--t1)}

/* 时间线 */
.cl .cl-tl{padding:6px 20px}
.cl .cl-tl li{display:grid;grid-template-columns:76px 18px 1fr;align-items:start;padding:10px 0}
.cl .cl-tl.wide li{grid-template-columns:100px 18px 1fr}
.cl .cl-tl li + li{border-top:1px solid var(--line)}
.cl .cl-tl .tm{font-size:13px;line-height:20px;color:var(--t3);white-space:nowrap}
.cl .cl-tl .cl-dot{width:8px;height:8px;margin-top:6px}
.cl .cl-tl .tx{font-size:13px;line-height:20px;color:var(--t1);min-width:0;overflow-wrap:anywhere}
.cl .cl-tl .tx small{display:block;font-size:12px;line-height:18px;color:var(--t3);margin-top:2px}
.cl .cl-tl li.empty{display:block;padding:14px 0;text-align:center}
.cl .cl-tl li.empty .tx{color:var(--t3)}

/* 运行日志 */
.cl .cl-xp{display:grid;grid-template-rows:0fr;transition:grid-template-rows var(--d-mid) var(--ease)}
.cl .cl-xp > div{overflow:hidden;min-height:0}
.cl .cl-xp.on{grid-template-rows:1fr}
.cl .cl-log{margin-top:12px;background:#2B3539;border:1px solid var(--line);border-radius:8px;padding:12px 16px;font:12px/18px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;color:#C9D3D6;height:240px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}

/* 设置行 */
.cl .cl-row + .cl-row{border-top:1px solid var(--line)}
.cl .cl-row-h{width:100%;display:flex;align-items:center;gap:12px;min-height:52px;padding:6px 20px;background:none;border:0;border-radius:0;color:inherit;text-align:left;cursor:pointer;font-size:13px;line-height:20px;transition:background var(--d-fast) var(--ease)}
.cl .cl-row:first-child .cl-row-h{border-radius:7px 7px 0 0}
.cl .cl-row:last-child:not(.open) .cl-row-h{border-radius:0 0 7px 7px}
.cl .cl-row:only-child:not(.open) .cl-row-h{border-radius:7px}
.cl .cl-row-h:hover{background:rgba(255,255,255,.04)}
.cl .cl-row-h .lb{flex:1;min-width:0}
.cl .cl-row-h .lb b{display:block;font-size:14px;line-height:20px;font-weight:500}
.cl .cl-row-h .lb span{display:block;font-size:12px;line-height:18px;color:var(--t3);margin-top:2px}
.cl .cl-row-h .val{font-size:13px;line-height:20px;color:var(--t2);white-space:nowrap;max-width:300px;overflow:hidden;text-overflow:ellipsis}
.cl .cl-chev{width:16px;height:16px;flex:none;color:var(--t3)}
.cl .cl-row.static .cl-row-h{cursor:default}
/* 国内网站直连下面的名单状态 */
.cl .cl-cnst{display:flex;align-items:baseline;gap:12px;margin-top:-2px;padding:0 20px 14px;font-size:12px;line-height:18px;color:var(--t3)}
.cl .cl-cnst[hidden]{display:none}
.cl .cl-cnst .tx{flex:1;min-width:0;overflow-wrap:anywhere}
.cl .cl-cnst.warn .tx{color:var(--warn)} .cl .cl-cnst.err .tx{color:var(--err)}
.cl .cl-row-h:disabled{cursor:default}
.cl .cl-row-h:disabled:hover{background:none}
.cl .cl-row-h:disabled .cl-chev{opacity:.35}
.cl .cl-row.static .cl-row-h:hover{background:none}
.cl .cl-row .cl-body{display:grid;grid-template-rows:0fr;transition:grid-template-rows var(--d-mid) var(--ease)}
.cl .cl-row .cl-body > div{overflow:hidden;min-height:0}
.cl .cl-row.open .cl-body{grid-template-rows:1fr}
.cl .cl-xp-in{padding:4px 20px 20px}

/* 表单 */
.cl .cl-fg{display:grid;grid-template-columns:96px 1fr;gap:12px 16px;align-items:center;max-width:520px}
.cl .cl-fg label{font-size:13px;line-height:20px;color:var(--t2)}
.cl .cl-in{height:32px;border-radius:6px;border:1px solid var(--input-line);background:var(--input);color:var(--t1);font-size:13px;line-height:20px;padding:0 10px;width:100%;transition:border-color var(--d-fast) var(--ease)}
.cl .cl-in::placeholder{color:#8E9A9E}
.cl .cl-in:focus{outline:none;border-color:var(--blue)}
.cl .cl-in.bad{border-color:var(--err)}
.cl select.cl-in{padding:0 6px}
.cl input[type=search].cl-in{-webkit-appearance:none;appearance:none}
.cl .cl-hint{font-size:12px;line-height:18px;color:var(--t3);margin-top:12px;word-break:keep-all;overflow-wrap:anywhere}
.cl .cl-acts{display:flex;align-items:center;gap:8px;margin-top:16px}
.cl .cl-msg{font-size:12px;line-height:18px;color:var(--t3);margin-left:4px}
.cl .cl-msg.err{color:var(--err)}
.cl .cl-pend{display:grid;grid-template-rows:0fr;transition:grid-template-rows var(--d-mid) var(--ease)}
.cl .cl-pend > div{overflow:hidden;min-height:0}
.cl .cl-pend.on{grid-template-rows:1fr}
.cl .cl-rowmsg{margin:8px 0 0}
.cl .cl-rowmsg:empty{display:none}
.cl .cl-body > div,.cl .cl-pend > div,.cl .cl-xp > div{visibility:hidden;transition:visibility 0s linear var(--d-mid)}
.cl .cl-row.open .cl-body > div,.cl .cl-pend.on > div,.cl .cl-xp.on > div{visibility:visible;transition-delay:0s}

.cl .cl-dev{display:grid;grid-template-columns:1fr auto;align-items:center;gap:12px;padding:10px 0}
.cl .cl-dev + .cl-dev{border-top:1px solid var(--line)}
.cl .cl-dev b{display:block;font-size:13px;line-height:20px;font-weight:500;overflow-wrap:anywhere}
.cl .cl-dev span{display:block;font-size:12px;line-height:18px;color:var(--t3)}
.cl .cl-dev .tag{font-size:12px;line-height:18px;color:var(--t3);text-align:right}
.cl .cl-dev.empty{display:block;color:var(--t3);font-size:13px;line-height:20px}
.cl .cl-dev > div:first-child{min-width:0}
.cl .cl-dev.off b{color:var(--t2)}
.cl .cl-dev .cl-seg{flex:none}
.cl .cl-dev-r{display:flex;align-items:center;gap:16px;flex:none}
.cl .cl-dev .cl-dev-p{color:var(--t2)}
/* 「直连端口」入口只在指到这一行或键盘移到这一行时出现，一列看下来不重复；已填端口的行一直显示「修改端口」 */
.cl .cl-dev .cl-lnk.pe{opacity:0;transition:opacity var(--d-fast) var(--ease),color var(--d-fast) var(--ease)}
.cl .cl-dev:hover .cl-lnk.pe,.cl .cl-dev:focus-within .cl-lnk.pe,.cl .cl-dev .cl-lnk.pe[aria-expanded="true"]{opacity:1}
@media (hover:none){.cl .cl-dev .cl-lnk.pe{opacity:1}}
/* 直连端口编辑：在设备行下面展开，占满整行 */
.cl .cl-pe{grid-column:1 / -1;padding:2px 0 2px}
.cl .cl-pe-in{display:flex;gap:8px;max-width:520px}
.cl .cl-pe-in .cl-in{flex:1;min-width:0}
.cl .cl-pe .cl-hint{margin-top:8px;max-width:560px}
.cl .cl-dname{display:block;max-width:100%;background:none;border:0;padding:6px 0;margin:-6px 0;color:var(--t1);font-size:13px;line-height:20px;font-weight:500;text-align:left;cursor:pointer;overflow-wrap:anywhere;word-break:break-all;border-radius:4px}
.cl .cl-dname:hover{text-decoration:underline;text-underline-offset:3px}
.cl .cl-tag{display:inline-block;vertical-align:top;margin:1px 0 0 8px;padding:0 6px;border:1px solid var(--line);border-radius:4px;font-style:normal;font-size:12px;line-height:14px;color:var(--t2)}
.cl .cl-subh{font-size:13px;line-height:20px;font-weight:500;color:var(--t2);margin-top:24px;padding-bottom:4px}
.cl .cl-xp-in > .cl-subh:first-child{margin-top:4px}
.cl .cl-more{display:inline-block;margin-top:12px}
.cl .cl-add{display:flex;gap:8px;margin-top:12px;max-width:520px}
.cl .cl-add .cl-in{flex:1;min-width:0}
.cl .cl-add .cl-seg{flex:none}

.cl .cl-seg{display:inline-flex;border:1px solid var(--input-line);border-radius:6px;overflow:hidden}
.cl .cl-seg button{height:30px;padding:0 14px;background:transparent;border:0;color:var(--t2);font-size:13px;line-height:20px;cursor:pointer;transition:background var(--d-fast) var(--ease),color var(--d-fast) var(--ease)}
.cl .cl-seg button:hover{color:var(--t1)}
.cl .cl-seg button + button{border-left:1px solid var(--input-line)}
.cl .cl-seg button[aria-checked="true"]{background:var(--blue);color:#fff}
/* 列表里每行的线路选择：选中态用中性底，一列看下来不刺眼 */
.cl .cl-seg.q button[aria-checked="true"]{background:#66757A;color:#fff}

/* 页脚 */
.cl .cl-foot{display:flex;justify-content:space-between;align-items:center;margin-top:24px;font-size:12px;line-height:18px;color:var(--t3)}

@media (prefers-reduced-motion:reduce){
  .cl *,.cl *::before,.cl *::after{transition:none!important;animation:none!important}
}
</style>
<script type="text/javascript">
/* ES5 only：华硕后台在各种浏览器里打开，不用箭头函数、let/const、模板字符串 */
var CL = (function () {
	var ADMIN_IP = '<% login_ip_str(); %>';
	if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(ADMIN_IP)) ADMIN_IP = '';

	var DEFAULT_SERVER = 'vpn.xindong.com:8443';
	var LOCK_NAME = '管理这台路由器的电脑';   // 后端自动加锁定设备时用的名字
	var DOMAIN_NOTE = { 'apple.com': '苹果官网、商店链接', 'mzstatic.com': 'App Store 图片和页面资源' };
	var ISP = [['', '运营商 DNS（自动）'], ['223.5.5.5,223.6.6.6', '阿里 223.5.5.5'], ['119.29.29.29,119.28.28.28', '腾讯 119.29.29.29'], ['custom', '自定义…']];
	var POLL_MS = 5000, STALE_S = 30, TL_MAX = 6;

	var db = {};                 // 已保存的 dbus（corpline_ 前缀）
	var dbLoaded = false;
	var st = null, stAt = 0;     // 最近一次状态 JSON 与取到的时间（浏览器毫秒）
	var prevUpd = 0, updMovedAt = 0;
	var pending = null;          // {kind:'start'|'retry'|'resume', at, started}
	var inflight = 0;            // 正在等回复的 /_api/ 请求数；期间所有会改设置的按钮都先停用
	var net = { okAt: Date.now(), fails: 0 };   // 最近一次读到路由器的时间、之后连续失败的次数
	var polling = false, pollSoon = 0, waitPoll = false;
	var clients = null;          // {MAC:{name, ip, online}}，取不到为 null
	var clientsFailed = false;
	var shown = { key: null, act: null, desc: null, tl: null };
	var edit = {};               // 各设置行未保存的改动
	var logOpen = false, logTimer = null, pollTimer = null, polls = 0, fastUntil = 0;

	/* ---------- 小工具 ---------- */
	function $i(id) { return document.getElementById(id); }
	function esc(s) { return String(s == null ? '' : s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }
	function trim(s) { return String(s || '').replace(/^\s+|\s+$/g, ''); }
	function pad(n) { return n < 10 ? '0' + n : '' + n; }
	function hhmm(t) { var d = new Date(t * 1000); return pad(d.getHours()) + ':' + pad(d.getMinutes()); }
	function dayStart(d) { return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime(); }
	function whenLabel(t) {
		var d = new Date(t * 1000), today = dayStart(new Date()), day = dayStart(d);
		if (day === today) return '今天 ' + hhmm(t);
		if (day === today - 86400000) return '昨天 ' + hhmm(t);
		return (d.getMonth() + 1) + '月' + d.getDate() + '日 ' + hhmm(t);
	}
	function isOld(t) { return dayStart(new Date(t * 1000)) < dayStart(new Date()) - 86400000; }
	function dur(s) {
		s = Math.max(0, Math.floor(s));
		if (s < 60) return s + ' 秒';
		var m = Math.floor(s / 60);
		if (m < 60) return m + ' 分钟';
		var h = Math.floor(m / 60); m = m % 60;
		if (h < 24) return h + ' 小时' + (m ? ' ' + m + ' 分' : '');
		var d = Math.floor(h / 24); h = h % 24;
		return d + ' 天' + (h ? ' ' + h + ' 小时' : '');
	}
	function countdown(s) {
		s = Math.max(0, Math.ceil(s));
		if (s < 120) return s + ' 秒后';
		return Math.floor(s / 60) + ' 分 ' + pad(s % 60) + ' 秒后';
	}
	function bytes(n) {
		n = Number(n) || 0;
		if (n < 1e6) return (n < 5e4 ? '0' : (n / 1e6).toFixed(1)) + ' MB';
		if (n < 1e9) { var m = n / 1e6; return (m < 10 ? m.toFixed(1) : Math.round(m)) + ' MB'; }
		var g = n / 1e9; return (g < 100 ? g.toFixed(1) : Math.round(g)) + ' GB';
	}
	function thousands(n) { return String(Math.round(Number(n) || 0)).replace(/\B(?=(\d{3})+(?!\d))/g, ','); }
	function b64e(s) {
		var u = unescape(encodeURIComponent(s));
		if (window.btoa) return window.btoa(u);
		return window.Base64 ? Base64.encode(u) : u;
	}
	function b64d(s) {
		if (!s) return '';
		try { var raw = window.atob ? window.atob(s) : Base64.decode(s); try { return decodeURIComponent(escape(raw)); } catch (e) { return raw; } }
		catch (e2) { return ''; }
	}
	function srvNow() { return st && num(st.updated) ? num(st.updated) + (Date.now() - stAt) / 1000 : Date.now() / 1000; }
	function host() { return (db.corpline_server || DEFAULT_SERVER).replace(/^https?:\/\//, '').replace(/[:\/].*$/, ''); }
	function relaxed() { return db.corpline_sensitivity === 'relaxed'; }
	// 状态文件里的数字都先转成数字再拼进页面
	function num(v) { var n = Number(v); return isFinite(n) ? n : 0; }

	function xhr(method, url, body, cb) {
		var x = new XMLHttpRequest();
		x.open(method, url, true);
		if (body != null) x.setRequestHeader('Content-Type', 'application/x-www-form-urlencoded; charset=UTF-8');
		x.timeout = 15000;
		x.onreadystatechange = function () { if (x.readyState === 4) cb(x.status, x.responseText); };
		x.send(body == null ? null : body);
	}
	function getJSON(url, cb) {
		xhr('GET', url + (url.indexOf('?') < 0 ? '?' : '&') + '_=' + Date.now(), null, function (code, text) {
			var o = null;
			if (code >= 200 && code < 300) { try { o = JSON.parse(text); } catch (e) { o = null; } }
			cb(code, o, text);
		});
	}

	/* ---------- 读写 dbus ---------- */
	function loadDbus(cb) {
		getJSON('/_api/corpline_', function (code, o) {
			var good = !!(o && o.result && o.result[0]);
			if (good) { db = o.result[0]; dbLoaded = true; }
			reach(good);
			if (cb) cb(good);
		});
	}
	// 读路由器成功/失败（登录过期时会拿到登录页，不是 JSON，也算失败）
	function reach(ok) {
		if (ok) { net.okAt = Date.now(); net.fails = 0; } else net.fails++;
	}
	function unreachable() { return net.fails >= 2 && Date.now() - net.okAt > 10000; }
	// action：1 应用（会重连）  4 只保存（不重连）  5 发测试通知  2 立即重试  3 重新启用  0 关闭  6 更新中国服务名单
	// 只提交这次改动的字段：软件中心只写收到的键，没提交的设置保持路由器上的值
	function submit(action, over, cb) {
		var id = Math.floor(Math.random() * 100000000), f = '', k;
		if (over) { f = {}; for (k in over) if (over.hasOwnProperty(k)) f[k] = String(over[k]); }
		inflight++; renderBusy();
		xhr('POST', '/_api/', JSON.stringify({ id: id, method: 'corpline_config.sh', params: [action], fields: f }), function (code, text) {
			inflight = Math.max(0, inflight - 1);
			var ok = false, res = null;
			if (code >= 200 && code < 300) { try { res = String(JSON.parse(text).result); } catch (e) { res = null; } }
			// 测试通知：成功回请求 id，没有地址回 nourl，发送失败回 fail
			ok = res === String(id) || (action === 5 && (res === 'nourl' || res === 'fail'));
			if (ok && f) {
				for (var k in f) if (f.hasOwnProperty(k) && k !== 'corpline_password') db[k] = f[k];
				if (f.corpline_password) db.corpline_has_password = '1';
			}
			if (cb) cb(ok, res === String(id) ? 'ok' : res);
			renderBusy();
			if (ok && action !== 5) { setTimeout(function () { loadDbus(renderAll); }, 1500); fastUntil = Date.now() + 30000; pollIn(800); }
		});
	}
	// 当前守护进程的启动时间；关闭时写的状态里没有 started
	function curStarted() { return st && st.state !== 'off' && st.started ? st.started : 0; }
	function startPending(was) { return { kind: 'start', at: Date.now(), started: was }; }
	function renderBusy() {
		var on = inflight > 0, ids = ['clSave-account', 'clSave-devices', 'clSave-domains', 'clSave-notify', 'clSave-adv', 'clCn', 'clNtTest', 'clCnUpd'], i, el;
		for (i = 0; i < ids.length; i++) { el = $i(ids[i]); if (el) el.disabled = on || (ids[i] === 'clCn' && !dbLoaded); }
		el = $i('clActBtn');
		if (el) el.disabled = on || el.getAttribute('data-busy') === '1';
		renderHeader(viewKey());
	}
	function failText() { return '没有保存成功：路由器没有响应，请稍后再试。'; }

	/* ---------- 设备 ---------- */
	function parseDevices(s) {
		var out = [], lines = b64d(s).split(/\r?\n/), i, p;
		for (i = 0; i < lines.length; i++) {
			p = lines[i].split('\t');
			if (/^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(p[0] || ''))
				out.push({ mac: p[0].toUpperCase(), name: p[1] || '', locked: p[2] === '1', route: p[3] === 'corp' && p[2] !== '1' ? 'corp' : 'direct', ports: p[4] || '' });
		}
		return dedupe(out);
	}
	// 同一个 MAC 只留一行（不分大小写）；任一行锁定则保持锁定，名字取第一个非空的
	function dedupe(list) {
		var out = [], seen = {}, i, d, k;
		for (i = 0; i < list.length; i++) {
			d = list[i]; k = String(d.mac).toUpperCase();
			if (seen.hasOwnProperty(k)) {
				var o = out[seen[k]];
				if (d.locked) o.locked = true;
				if (!o.name || o.name === LOCK_NAME) o.name = d.name && d.name !== LOCK_NAME ? d.name : o.name;
				if (!o.ports) o.ports = d.ports || '';
				continue;
			}
			seen[k] = out.length;
			out.push({ mac: k, name: d.name || '', locked: !!d.locked, route: d.locked ? 'direct' : d.route || 'direct', ports: d.ports || '' });
		}
		// 锁定的管理电脑排在最前
		out.sort(function (a, b) { return (b.locked ? 1 : 0) - (a.locked ? 1 : 0); });
		return out;
	}
	function encodeDevices(list) {
		var lines = [], i, d;
		list = dedupe(list);
		for (i = 0; i < list.length; i++) {
			d = list[i];
			// 整台直连、没填端口的设备仍写老的三列格式，旧版本也能读
			lines.push(d.mac + '\t' + String(d.name || '').replace(/[\t\r\n]/g, ' ') + '\t' + (d.locked ? '1' : '0') +
				(d.route === 'corp' || d.ports ? '\t' + (d.route === 'corp' ? 'corp' : 'direct') + '\t' + d.ports : ''));
		}
		return lines.length ? b64e(lines.join('\n')) : '';
	}
	// "32400, 3981 3982-4081" → "32400,3981,3982-4081"；格式不对返回 null
	function normPorts(s) {
		var t = String(s || '').replace(/[，、；;]/g, ',').replace(/\s*[-–~]\s*/g, '-').split(/[,\s]+/), out = [], seen = {}, i, m, a, b, k;
		for (i = 0; i < t.length; i++) {
			if (!t[i]) continue;
			m = /^(\d{1,5})(?:-(\d{1,5}))?$/.exec(t[i]);
			if (!m) return null;
			a = +m[1]; b = m[2] ? +m[2] : a;
			if (a < 1 || b > 65535 || a > b) return null;
			k = a === b ? String(a) : a + '-' + b;
			if (!seen[k]) { seen[k] = 1; out.push(k); }
		}
		return out.length > 20 ? null : out.join(',');
	}
	function portsText(p) { return String(p || '').split(',').join('、').replace(/-/g, '–'); }
	function loadClients() {
		getJSON('/appGet.cgi?hook=get_clientlist()', function (code, o) {
			var src = o && (o.get_clientlist || o), k, c, map = {}, n = 0;
			if (src && typeof src === 'object') {
				for (k in src) {
					if (!src.hasOwnProperty(k) || !/^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(k)) continue;
					c = src[k] || {};
					map[k.toUpperCase()] = { name: trim(c.nickName) || trim(c.name) || '', ip: c.ip || '', online: c.isOnline === undefined ? true : String(c.isOnline) === '1' };
					n++;
				}
			}
			if (n) { clients = map; clientsFailed = false; } else { clients = null; clientsFailed = true; }
			renderDevices();
		});
	}
	function clientByIp(ip) {
		if (!clients || !ip) return null;
		for (var k in clients) if (clients.hasOwnProperty(k) && clients[k].ip === ip) return { mac: k, name: clients[k].name };
		return null;
	}

	/* ---------- 状态推导 ---------- */
	function fresh() {
		if (!st || !st.updated) return false;
		return (Date.now() / 1000 - st.updated <= STALE_S) || (updMovedAt && Date.now() - updMovedAt <= STALE_S * 1000);
	}
	function viewKey() {
		if (unreachable()) return 'noreach';
		if (!dbLoaded) return 'loading';
		if (db.corpline_paused === '1') return 'paused';
		if (!trim(db.corpline_user) || db.corpline_has_password !== '1') return 'setup';
		if (db.corpline_enable !== '1') return 'off';
		var live = fresh() && st.state && st.state !== 'off';
		// 会重启守护进程的操作：等到新进程（started 变了）写出状态才算启动完，最多等 45 秒
		if (pending && pending.kind === 'start') {
			var restarted = st && st.started ? st.started !== pending.started : st && num(st.updated) * 1000 > pending.at + 2000;
			if (live && restarted) pending = null;
			else if (Date.now() - pending.at < 45000) return 'starting';
			else pending = null;
		}
		if (!live) return 'down';
		return { connecting: 1, active: 1, watching: 1, fallback: 1, autherr: 1 }[st.state] ? st.state : 'down';
	}

	function metric(l, v, s, wrap) { return { l: l, v: v, s: s || '', wrap: !!wrap }; }
	function trafficHtml() {
		var t = st && st.traffic_today || {};
		return '↓ ' + bytes(t.rx) + '<span class="gap"></span>↑ ' + bytes(t.tx);
	}
	function dnsMetric() {
		var d = st && st.dns || {};
		if (d.company === false) return metric('域名解析', '备用 DNS', '公司 DNS 暂时没响应');
		return metric('域名解析', '公司 DNS', d.queries ? '已处理 ' + thousands(d.queries) + ' 次查询' : '');
	}
	function spec(key) {
		var now = srvNow(), r = st && st.retry || {}, since = num(st && st.since), s;
		var tries = num(r.attempts_today), cap = num(r.daily_cap) || 60, rtt = st && st.rtt_ms != null && st.rtt_ms !== '' && isFinite(Number(st.rtt_ms)) ? Math.round(Number(st.rtt_ms)) : null;
		var direct = metric('当前线路', '运营商直连', '家里宽带');
		switch (key) {
		case 'loading':
			return { dot: 'idle', title: '正在读取状态…', desc: '', m: [] };
		case 'active':
			s = st.cn_direct ? '全家上网都经过公司线路；淘宝、B 站这类国内网站直连家里宽带。' : '全家上网都经过公司线路，国内外网站由公司线路自动分流。';
			return { dot: 'ok', title: '正在使用公司专线', desc: s, m: [
				metric('已稳定', since ? dur(now - since) : '—'),
				metric('今日流量', trafficHtml()),
				metric('延迟', rtt != null ? rtt + ' ms' : '—', '经公司 DNS 测得'),
				dnsMetric()] };
		case 'watching':
			return { dot: '', pulse: true, title: '公司线路有点慢，正在观察', desc: '仍在使用公司专线。如果持续不稳，会自动切到直连，不需要你处理。', m: [
				metric('延迟', rtt != null ? rtt + ' ms' : '没有响应', '超过 ' + (relaxed() ? 1500 : 800) + ' ms 算偏慢'),
				metric('已观察', since ? dur(now - since) : '—', '超过 ' + (relaxed() ? 75 : 45) + ' 秒切到直连'),
				metric('今日流量', trafficHtml()),
				dnsMetric()] };
		case 'fallback':
			var reason = st.reason || '', hold = relaxed() ? '3 分钟' : '2 分钟', at = since ? hhmm(since) : '刚才';
			var recovering = st.tunnel && st.tunnel.up && /恢复/.test(reason);
			var capped = /上限/.test(reason) || (num(r.daily_cap) && tries >= cap);
			if (recovering) s = '公司线路已经恢复，再稳定 ' + hold + '就自动切回。家里照常上网。';
			else if (capped) s = '今天重连次数已到上限，先保持直连，明天自动再试；也可以点「立即重试」。家里照常上网。';
			else s = (/断开/.test(reason) ? '公司线路在 ' + at + ' 断开了' : /连不上/.test(reason) ? '从 ' + at + ' 起暂时连不上公司线路' : '公司线路从 ' + at + ' 起没有响应') +
				'。家里照常上网，只是暂时不经过公司线路；会自动重试，恢复稳定 ' + hold + '后自动切回。';
			var next;
			if (recovering) next = metric('公司线路', '已恢复', '稳定 ' + hold + '后切回');
			else if (r.connecting || pending && pending.kind === 'retry') next = metric('下次重试', '正在重连');
			else if (capped) next = metric('下次重试', '明天', '今天的次数已用完');
			else if (num(r.next_in) > 0) next = metric('下次重试', countdown(num(r.next_in) - (Date.now() - stAt) / 1000));
			else next = metric('下次重试', '马上');
			return { dot: 'warn', title: '已临时切到直连', desc: s,
				act: { cls: 'sec', label: pending && pending.kind === 'retry' ? '正在重试…' : '立即重试', busy: !!(pending && pending.kind === 'retry'), fn: retry },
				m: [direct, metric('已持续', since ? dur(now - since) : '—'), next,
					metric('今日重试', tries + ' / ' + cap + ' 次', '到上限后明天再试')] };
		case 'autherr':
			return { dot: 'err', title: '公司账号登录失败',
				desc: '公司服务器拒绝了这个账号和密码，可能是密码改了。为了不让账号被锁，已停止自动重试；家里已切回直连，照常上网。',
				act: { cls: 'pri', label: '更新密码', fn: function () { openRow('r-account', true); focusLater('f-pw'); } },
				m: [direct, metric('停止重试', since ? hhmm(since) : '—'), metric('账号', esc(db.corpline_user || '—'), '', true), metric('服务器', esc(host()), '', true)] };
		case 'connecting':
		case 'starting':
			return { dot: '', pulse: true, title: '正在连接公司线路…', desc: '连接成功、确认线路正常之前，家里保持直连。',
				m: key === 'starting' ? [direct, metric('已用时', dur((Date.now() - pending.at) / 1000)), metric('服务器', esc(host()), '', true)] :
					[direct, metric('已用时', since ? dur(now - since) : '—'), metric('尝试', '今天第 ' + Math.max(1, tries) + ' 次'), metric('服务器', esc(host()), '', true)] };
		case 'paused':
			var pr = trim(db.corpline_paused_reason).replace(/[，,]?\s*为安全起见先停用\s*$/, '').replace(/[。.]$/, '');
			if (/^\d+ 分钟内意外退出/.test(pr)) pr = '连接程序 ' + pr.replace('意外退出', '意外退出了');
			return { dot: 'err', title: '插件运行异常，已自动停用',
				desc: (pr ? pr + '。' : '') + '为了不影响上网，已自动停用，全家直连。可以先看运行日志，再重新启用。',
				act: { cls: 'pri', label: pending && pending.kind === 'resume' ? '正在启用…' : '重新启用', busy: !!(pending && pending.kind === 'resume'), fn: resume }, m: [] };
		case 'down':
			return { dot: 'warn', title: '插件没有在运行', desc: '全家暂时直连上网。路由器每分钟会检查一次并自动拉起；如果一直是这样，可以先看运行日志，再点「重新启动」。',
				act: { cls: 'sec', label: '重新启动', fn: restart }, m: [] };
		case 'noreach':
			return { dot: 'idle', title: '暂时读不到路由器状态',
				desc: '页面暂时连不上路由器，可能是网络断了一下，或者后台登录已过期。线路本身不受影响，页面会自动重试；一直这样的话，刷新页面或重新登录路由器后台。', m: [] };
		case 'setup':
			return { dot: 'idle', title: '先填写公司账号', desc: '填好公司 VPN 的服务器、账号和密码并保存，就可以打开右上角的开关。',
				act: { cls: 'pri', label: '填写账号', fn: function () { openRow('r-account', true); focusLater(trim($i('f-user').value) ? 'f-pw' : 'f-user'); } }, m: [] };
		default:
			return { dot: 'idle', title: '已关闭', desc: '全家直连上网，没有经过公司线路。打开右上角开关即可开始使用。', m: [] };
		}
	}

	/* ---------- 渲染 ---------- */
	function renderStatus() {
		var key = viewKey(), s = spec(key), body = $i('clStBody');
		var changed = shown.key !== key;
		function paint() {
			var dot = $i('clStDot');
			if (changed) {
				dot.className = 'cl-dot' + (s.dot ? ' ' + s.dot : '');
				if (s.pulse) { void dot.offsetWidth; dot.className += ' pulse'; }
				$i('clStTitle').textContent = s.title;
			}
			if (shown.desc !== s.desc) { $i('clStDesc').textContent = s.desc; $i('clStDesc').hidden = !s.desc; shown.desc = s.desc; }
			var actSig = s.act ? s.act.cls + '|' + s.act.label + '|' + !!s.act.busy : '';
			if (shown.act !== actSig) {
				var box = $i('clStAct');
				box.innerHTML = s.act ? '<button type="button" class="cl-btn ' + s.act.cls + '" id="clActBtn" data-busy="' + (s.act.busy ? 1 : 0) + '"' + (s.act.busy || inflight ? ' disabled' : '') + '>' + esc(s.act.label) + '</button>' : '';
				if (s.act) $i('clActBtn').onclick = s.act.fn;
				shown.act = actSig;
			}
			var h = '', i, m;
			for (i = 0; i < s.m.length; i++) {
				m = s.m[i];
				h += '<div class="cl-mt"><div class="l">' + m.l + '</div><div class="v cl-num' + (m.wrap ? ' wrap' : '') + '">' + m.v + '</div>' + (m.s ? '<div class="s">' + m.s + '</div>' : '') + '</div>';
			}
			var mt = $i('clMetrics');
			if (mt.innerHTML !== h) mt.innerHTML = h;
		}
		if (changed && shown.key !== null && shown.key !== 'loading') {
			shown.key = key;
			body.className = 'cl-st-body fade';
			setTimeout(function () { paint(); body.className = 'cl-st-body'; }, 150);
		} else { paint(); shown.key = key; }
		renderHeader(key);
		renderFoot(key);
	}
	function renderHeader(key) {
		var m = $i('clMaster'), lab = $i('clSwLabel');
		if (m.className.indexOf('busy') >= 0) { m.disabled = true; return; }
		// 自动停用时开关仍是开着的（用户本来开着），点它就是关掉插件；恢复用状态卡里的「重新启用」
		var on = key === 'noreach' ? db.corpline_enable === '1' : key !== 'setup' && key !== 'off' && key !== 'loading';
		m.setAttribute('aria-checked', on ? 'true' : 'false');
		m.disabled = key === 'setup' || key === 'loading' || key === 'noreach' || inflight > 0;
		lab.textContent = key === 'setup' ? '未设置' : key === 'paused' ? '已自动停用' : key === 'loading' || key === 'noreach' ? '' : on ? '已开启' : '已关闭';
	}
	function renderFoot(key) {
		var ver = (st && st.version) || db.corpline_version || '';
		var running = fresh() && st.state && st.state !== 'off' && st.res && key !== 'off' && key !== 'paused' && key !== 'setup' && key !== 'noreach';
		var t = (ver ? '版本 ' + ver + ' · ' : '');
		if (running) t += '内存 ' + ((num(st.res.mem_kb) + num(st.res.oc_mem_kb)) / 1024).toFixed(1) + ' MB · CPU ' + num(st.res.cpu_pct) + '%';
		else t += key === 'noreach' ? '运行情况未知' : '未运行';
		if (shown.foot !== t) { $i('clFoot').textContent = t; shown.foot = t; }
	}
	function renderTimeline() {
		var ev = (st && st.events) || [], h = '', i, e, wide = false, n = Math.min(ev.length, TL_MAX), lv;
		for (i = 0; i < n; i++) if (isOld(num(ev[i].t))) wide = true;
		for (i = 0; i < n; i++) {
			e = ev[i];
			lv = e.l === 'ok' ? 'ok' : e.l === 'warn' ? 'warn' : e.l === 'err' ? 'err' : '';
			var parts = String(e.m || '').split('\n');
			h += '<li><span class="tm cl-num">' + whenLabel(num(e.t)) + '</span><span class="cl-dot ' + lv + '"></span><span class="tx">' + esc(parts[0]) +
				(parts[1] ? '<small>' + esc(parts.slice(1).join(' ')) + '</small>' : '') + '</span></li>';
		}
		if (!h) h = '<li class="empty"><span class="tx">' + (!st && unreachable() ? '暂时读不到记录，连上路由器后会自动显示。' : '还没有记录。开始使用后，线路切换和异常都会记在这里。') + '</span></li>';
		h = (wide ? 'w' : 'n') + h;
		if (shown.tl === h) return;
		shown.tl = h;
		var ul = $i('clTimeline');
		ul.className = 'cl-card cl-tl' + (wide ? ' wide' : '');
		ul.innerHTML = h.slice(1);
	}

	/* 设置行 */
	function setVal(id, t) { var el = $i(id); if (el.textContent !== t) el.textContent = t; }
	function renderAccount() {
		var u = trim(db.corpline_user);
		setVal('clAcctVal', u ? u + ' · ' + host() : '未填写');
		if (!edit.account) {
			$i('f-srv').value = db.corpline_server || DEFAULT_SERVER;
			$i('f-user').value = db.corpline_user || '';
		}
		$i('f-pw').placeholder = db.corpline_has_password === '1' ? '已保存，留空则不修改' : '公司 VPN 密码';
		$i('clAcctHint').textContent = '密码只保存在路由器上，页面里不会再显示。' + (db.corpline_enable === '1' ?
			'保存后会重新连接公司线路，连上并确认正常之前，家里保持直连。' : '保存后打开右上角开关即可开始使用。');
		$i('clSave-account').textContent = db.corpline_enable === '1' ? '保存并重新连接' : '保存';
	}
	/* 家里的设备：在线设备 + 直连名单里不在线的设备，每台选走哪条线 */
	function devList() { return edit.devices || parseDevices(db.corpline_devices); }
	function devName(d, c) { return (d && d.name && d.name !== LOCK_NAME && d.name) || (c && c.name) || ''; }
	// 按名字排：英文名在前（不分大小写），中文名其次，没有名字的最后
	function byName(a, b) {
		function grp(n) { return !n ? 2 : /^[\x00-\x7f]/.test(n) ? 0 : 1; }
		var ga = grp(a.name), gb = grp(b.name), x, y;
		if (ga !== gb) return ga - gb;
		x = ga === 2 ? a.mac : a.name.toLowerCase(); y = gb === 2 ? b.mac : b.name.toLowerCase();
		try { return x.localeCompare(y, 'zh-Hans-CN'); } catch (e) { return x < y ? -1 : x > y ? 1 : 0; }
	}
	function devRows() {
		var list = devList(), saved = parseDevices(db.corpline_devices), rule = {}, seen = {}, pinned = [], online = [], offline = [], i, k, d, c, nd = 0, np = 0;
		for (i = 0; i < list.length; i++) {
			rule[list[i].mac] = list[i];
			if (list[i].route === 'corp') { if (list[i].ports) np++; } else nd++;
		}
		function isDirect(m) { return !!rule[m] && rule[m].route !== 'corp'; }
		function portsOf(m) { return rule[m] && rule[m].route === 'corp' ? rule[m].ports : ''; }
		// 管理电脑：已锁定的那行；还没开启过就用登录后台的这台电脑占位，开启时自动加入
		for (i = 0; i < list.length; i++) if (list[i].locked) {
			c = clients && clients[list[i].mac];
			pinned.push({ mac: list[i].mac, name: devName(list[i], c) || '这台电脑', ip: c && c.ip || '', online: !c || c.online, locked: true });
			seen[list[i].mac] = 1;
		}
		if (!pinned.length && ADMIN_IP) {
			var me = clientByIp(ADMIN_IP);
			pinned.push({ mac: me ? me.mac : '', name: me && me.name || '这台电脑', ip: ADMIN_IP, online: true, locked: true, later: true });
			if (me) seen[me.mac] = 1;
		}
		if (clients) for (k in clients) if (clients.hasOwnProperty(k) && clients[k].online && !seen[k]) {
			online.push({ mac: k, name: devName(rule[k], clients[k]), ip: clients[k].ip, online: true, direct: isDirect(k), ports: portsOf(k) });
			seen[k] = 1;
		}
		// 不在线但在直连名单里的设备（含刚切回公司线路、还没保存的），规则不能丢
		var cand = list.concat(saved);
		for (i = 0; i < cand.length; i++) {
			d = cand[i]; if (seen[d.mac] || d.locked) continue;
			c = clients && clients[d.mac];
			offline.push({ mac: d.mac, name: devName(rule[d.mac] || d, c), ip: c && c.ip || '', online: false, unknown: !clients, direct: isDirect(d.mac), ports: portsOf(d.mac) });
			seen[d.mac] = 1;
		}
		online.sort(byName); offline.sort(byName);
		return { rows: pinned.concat(online, offline), online: online.length + (pinned.length && pinned[0].online && clients && pinned[0].ip ? 1 : 0), direct: nd + (pinned.length && pinned[0].later ? 1 : 0), partial: np };
	}
	function seg(label, cur, opts, attr) {
		var h = '<div class="cl-seg q" role="radiogroup" aria-label="' + esc(label) + '">', i;
		for (i = 0; i < opts.length; i++)
			h += '<button type="button" role="radio" aria-checked="' + (opts[i][0] === cur) + '" data-v="' + opts[i][0] + '"' + attr + '>' + opts[i][1] + '</button>';
		return h + '</div>';
	}
	function renderDevices() {
		if (!dbLoaded) { renderLock(false); return; }
		var r = devRows(), h = '', i, d, sub, right;
		for (i = 0; i < r.rows.length; i++) {
			d = r.rows[i];
			sub = [d.ip, d.mac].filter(function (x) { return !!x; }).join(' · ');
			if (!d.online && !d.unknown) sub += ' · 不在线';
			if (d.locked) {
				right = '<span class="tag">固定直连</span>';
				h += '<div class="cl-dev"><div><b>' + esc(d.name) + '</b><span class="cl-num">' + esc(sub) + '</span><span>' +
					(d.later ? '管理这台路由器的电脑，开启时自动加入直连，保证随时能打开后台' : '管理这台路由器的电脑，保证随时能打开后台') + '</span></div>' + right + '</div>';
			} else {
				var open = portsOpen === d.mac && !d.direct;
				right = '<div class="cl-dev-r">' + (d.direct ? '' : '<button type="button" class="cl-lnk' + (d.ports ? '' : ' pe') + '" data-pmac="' + d.mac + '" aria-expanded="' + open + '">' + (d.ports ? '修改端口' : '直连端口') + '</button>') +
					seg((d.name || d.mac) + ' 走哪条线', d.direct ? 'direct' : 'corp', [['corp', '公司线路'], ['direct', '直连']], ' data-mac="' + d.mac + '"') + '</div>';
				h += '<div class="cl-dev' + (d.online ? '' : ' off') + '"><div><b>' + esc(d.name || '未命名设备') + '</b><span class="cl-num">' + esc(sub) + '</span>' +
					(d.ports ? '<span class="cl-num cl-dev-p">' + esc(portsText(d.ports)) + ' 端口直连，其余走公司线路</span>' : '') + '</div>' + right +
					(open ? '<div class="cl-pe"><div class="cl-pe-in"><input class="cl-in cl-num" id="clDevPorts" value="' + esc(portsText(d.ports).replace(/、/g, ', ').replace(/–/g, '-')) +
						'" placeholder="例如 32400, 3981, 3982-4081" aria-label="' + esc((d.name || d.mac) + ' 直连的端口') + '" spellcheck="false" data-pmac="' + d.mac + '">' +
						'<button type="button" class="cl-btn sec" id="clDevPortsOk" data-pmac="' + d.mac + '">完成</button></div>' +
						'<p class="cl-hint">填这台设备上程序用的端口，用逗号隔开，可以写一段（3982-4081）。从这些端口发出的连接直连，其余仍走公司线路。清空后点完成即取消。</p></div>' : '') + '</div>';
			}
		}
		if (!h) h = '<div class="cl-dev empty">读不到在线设备。家里的设备默认都走公司线路。</div>';
		$i('clDevList').innerHTML = h;
		setVal('clDevVal', (clients ? r.online + ' 台在线 · ' : '') + r.direct + ' 台直连' + (r.partial ? ' · ' + r.partial + ' 台部分端口直连' : ''));
		var btns = $i('clDevList').querySelectorAll('button[data-mac]');
		for (i = 0; i < btns.length; i++) btns[i].onclick = setDeviceRoute;
		btns = $i('clDevList').querySelectorAll('.cl-lnk[data-pmac]');
		for (i = 0; i < btns.length; i++) btns[i].onclick = togglePorts;
		var pin = $i('clDevPorts');
		if (pin) {
			$i('clDevPortsOk').onclick = savePorts;
			pin.onkeydown = function (e) {
				var k = (e || window.event).keyCode;
				if (k === 13) savePorts();
				else if (k === 27) { portsOpen = ''; msg('devices', ''); renderDevices(); focusPortsLink(this.getAttribute('data-pmac')); }
			};
		}
		// 不在线的设备手动填 MAC；读不到在线设备时直接展开
		var manual = clientsFailed || devManual;
		$i('clDevManual').hidden = !manual;
		$i('clDevManualBtn').hidden = manual;
		$i('clDevNote').hidden = !manual;
		$i('clDevNote').textContent = (clientsFailed ? '读不到路由器的在线设备列表，请手动填写。' : '') + 'MAC 地址形如 AA:BB:CC:DD:EE:FF，可以在「网络地图 › 客户端」里找到。';
		setPend('devices', !!edit.devices && encodeDevices(edit.devices) !== (db.corpline_devices || ''));
	}
	var devManual = false, portsOpen = '';
	// 改一台设备的规则（线路或直连端口）；走公司线路又没填端口的设备不用记
	function setDevRule(mac, route, ports) {
		var list = devList(), out = [], i, d = null, c = clients && clients[mac];
		for (i = 0; i < list.length; i++) if (list[i].mac === mac) d = list[i]; else out.push(list[i]);
		if (!d) {
			var keep = parseDevices(db.corpline_devices), nm = c && c.name || '';
			for (i = 0; i < keep.length; i++) if (keep[i].mac === mac && keep[i].name) nm = keep[i].name;
			d = { mac: mac, name: nm, locked: false, route: 'corp', ports: '' };
		}
		d = { mac: d.mac, name: d.name, locked: d.locked, route: route || d.route, ports: ports === undefined ? d.ports : ports };
		if (d.route === 'direct' || d.ports) out.push(d);
		edit.devices = out;
	}
	function focusPortsLink(mac) {
		var el = $i('clDevList').querySelector('.cl-lnk[data-pmac="' + mac + '"]');
		if (el) try { el.focus({ preventScroll: true }); } catch (e) { el.focus(); }
	}
	function togglePorts() {
		var mac = this.getAttribute('data-pmac');
		portsOpen = portsOpen === mac ? '' : mac; msg('devices', ''); renderDevices();
		if (portsOpen) { var el = $i('clDevPorts'); if (el) { el.focus(); el.setSelectionRange && el.setSelectionRange(el.value.length, el.value.length); } }
		else focusPortsLink(mac);
	}
	function savePorts() {
		var el = $i('clDevPorts'), mac = el.getAttribute('data-pmac'), p = normPorts(el.value);
		if (p === null) { bad('clDevPorts', 'devices', '端口要写成 1–65535 之间的数字，一段写成 3982-4081，最多 20 项。'); return; }
		setDevRule(mac, null, p);
		portsOpen = ''; msg('devices', ''); renderDevices(); focusPortsLink(mac);
	}
	function setDeviceRoute() {
		var mac = this.getAttribute('data-mac'), want = this.getAttribute('data-v');
		if (want === 'direct' && portsOpen === mac) portsOpen = '';
		setDevRule(mac, want);
		msg('devices', ''); renderDevices();
		var again = $i('clDevList').querySelector('button[data-mac="' + mac + '"][data-v="' + want + '"]');
		if (again) try { again.focus({ preventScroll: true }); } catch (e) { again.focus(); }
	}
	function addDevice() {
		var list = devList().slice(), mac, name, i;
		mac = trim($i('clDevMac').value).toUpperCase().replace(/-/g, ':');
		if (/^[0-9A-F]{12}$/.test(mac)) mac = mac.replace(/(..)(?!$)/g, '$1:');
		name = trim($i('clDevName').value);
		if (!/^([0-9A-F]{2}:){5}[0-9A-F]{2}$/.test(mac)) { bad('clDevMac', 'devices', 'MAC 地址的格式不对。'); return; }
		for (i = 0; i < list.length; i++) if (list[i].mac === mac && list[i].route !== 'corp') { bad('clDevMac', 'devices', '这台设备已经设成直连了。'); return; }
		if (clients && clients[mac] && clients[mac].online && !name) name = clients[mac].name;
		setDevRule(mac, 'direct');
		if (name) for (list = devList(), i = 0; i < list.length; i++) if (list[i].mac === mac) list[i].name = name;
		msg('devices', '');
		$i('clDevMac').value = ''; $i('clDevName').value = '';
		renderDevices();
	}

	/* 网站单独设置：直连 / 海外解析两张名单，同一个域名只能在一张里 */
	var ROUTE = { direct: '直连', overseas: '海外解析', corp: '公司线路', internal: '公司内网' };
	function splitList(s) { return String(s || '').split(/[,\s]+/).filter(function (x) { return !!x; }); }
	function savedRules() {
		var out = [], seen = {}, i, a = splitList(db.corpline_direct_domains), b = splitList(db.corpline_overseas_domains);
		for (i = 0; i < a.length; i++) if (!seen[a[i]]) { seen[a[i]] = 1; out.push({ d: a[i], r: 'direct' }); }
		for (i = 0; i < b.length; i++) if (!seen[b[i]]) { seen[b[i]] = 1; out.push({ d: b[i], r: 'overseas' }); }
		return out;
	}
	function domRules() { return edit.domains || savedRules(); }
	function rulesKey(list, r) { var o = [], i; for (i = 0; i < list.length; i++) if (list[i].r === r) o.push(list[i].d); return o.join(','); }
	function ruleFor(d) {
		var list = domRules(), best = null, i;
		for (i = 0; i < list.length; i++)
			if ((d === list[i].d || d.slice(-list[i].d.length - 1) === '.' + list[i].d) && (!best || list[i].d.length > best.d.length)) best = list[i];
		return best;
	}
	function setRule(d, r) {
		var list = domRules().slice(), i, found = false;
		for (i = 0; i < list.length; i++) if (list[i].d === d) { list[i] = { d: d, r: r }; found = true; }
		if (!found && r) list.push({ d: d, r: r });
		if (!r) list = list.filter(function (x) { return x.d !== d; });
		edit.domains = list; msg('domains', '');
		renderDomains(); renderRecent();
	}
	function renderDomains() {
		if (!dbLoaded) { renderLock(false); return; }
		var list = domRules(), h = '', i, x, n = { direct: 0, overseas: 0 };
		for (i = 0; i < list.length; i++) {
			x = list[i]; n[x.r]++;
			h += '<div class="cl-dev"><div><b>' + esc(x.d) + '</b>' + (DOMAIN_NOTE[x.d] ? '<span>' + DOMAIN_NOTE[x.d] + '</span>' : '') + '</div>' +
				'<div class="cl-dev-r">' + seg(x.d + ' 怎么走', x.r, [['direct', '直连'], ['overseas', '海外解析']], ' data-dom="' + esc(x.d) + '"') +
				'<button type="button" class="cl-lnk" data-rm="' + esc(x.d) + '">移除</button></div></div>';
		}
		if (!h) h = '<div class="cl-dev empty">没有单独设置的网站，都按默认走公司线路和公司 DNS。</div>';
		$i('clDomList').innerHTML = h;
		var b = $i('clDomList').getElementsByTagName('button'), j;
		for (j = 0; j < b.length; j++) {
			if (b[j].getAttribute('data-rm')) b[j].onclick = function () { setRule(this.getAttribute('data-rm'), ''); };
			else b[j].onclick = function () { setRule(this.getAttribute('data-dom'), this.getAttribute('data-v')); };
		}
		var v = [];
		if (n.direct) v.push('直连 ' + n.direct);
		if (n.overseas) v.push('海外解析 ' + n.overseas);
		setVal('clDomVal', v.length ? v.join(' · ') : '无');
		var segs = $i('clDomAddSeg').getElementsByTagName('button');
		for (j = 0; j < segs.length; j++) segs[j].setAttribute('aria-checked', segs[j].getAttribute('data-v') === addRoute ? 'true' : 'false');
		var s = savedRules();
		setPend('domains', !!edit.domains && (rulesKey(list, 'direct') !== rulesKey(s, 'direct') || rulesKey(list, 'overseas') !== rulesKey(s, 'overseas')));
	}
	var addRoute = 'direct';
	function addDomain() {
		var v = trim($i('clDomIn').value).toLowerCase().replace(/^[a-z]+:\/\//, '').replace(/[\/?#:].*$/, '').replace(/^\*\./, '').replace(/^www\./, '').replace(/\.$/, '');
		if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(v)) { bad('clDomIn', 'domains', '请填网址里的域名部分，例如 nintendo.co.jp。'); return; }
		var list = domRules(), i;
		for (i = 0; i < list.length; i++) if (list[i].d === v && list[i].r === addRoute) { bad('clDomIn', 'domains', v + ' 已经设成' + ROUTE[addRoute] + '了。'); return; }
		$i('clDomIn').value = '';
		setRule(v, addRoute);
	}

	/* 最近访问的网站：面板打开、页面在前台时每 5 秒读一次 */
	var rec = { code: 0, items: [], updated: 0, at: 0 }, recLimit = 30, recTimer = null, recBusy = false;
	function recOpen() { return $i('r-global').className.indexOf(' open') >= 0 && !document.hidden; }
	function syncRecent() {
		clearTimeout(recTimer); recTimer = null;
		if (!recOpen()) return;
		if (!recBusy) loadRecent();
	}
	function loadRecent() {
		recBusy = true;
		getJSON('/_temp/corpline_domains.json', function (code, o) {
			recBusy = false;
			if (o && o.items && o.items.length !== undefined) { rec = { code: 200, items: o.items, updated: num(o.updated), at: Date.now() }; }
			else rec = { code: code === 404 ? 404 : -1, items: rec.code === 200 && code !== 404 ? rec.items : [], updated: rec.updated, at: rec.at };
			renderRecent();
			clearTimeout(recTimer);
			recTimer = recOpen() ? setTimeout(loadRecent, POLL_MS) : null;
		});
	}
	function ago(t) {
		var s = (rec.updated || Date.now() / 1000) - num(t) + (Date.now() - rec.at) / 1000;
		if (s < 60) return '刚刚';
		if (s < 3600) return Math.floor(s / 60) + ' 分钟前';
		if (s < 86400) return Math.floor(s / 3600) + ' 小时前';
		return Math.floor(s / 86400) + ' 天前';
	}
	function renderRecent() {
		var box = $i('clRecList'), q = trim($i('clRecQ').value).toLowerCase(), h = '', i, it, d, shown = 0, total = 0, r, note = '';
		if (rec.code === 404) note = '插件运行时才有记录。';
		else if (rec.code === -1 && !rec.items.length) note = '暂时读不到最近访问的网站，稍后会自动再试。';
		else if (rec.code === 200 && !rec.items.length) note = '还没有记录。家里有设备上网后，这里会列出最近访问的网站。';
		for (i = 0; i < rec.items.length && !note; i++) {
			it = rec.items[i] || {}; d = String(it.d || '').toLowerCase();
			if (!d || (q && d.indexOf(q) < 0)) continue;
			total++;
			if (shown >= recLimit) continue;
			shown++;
			r = ruleFor(d);
			var c = ROUTE[it.c] || '公司线路';
			h += '<div class="cl-dev"><div><button type="button" class="cl-dname" data-copy="' + esc(d) + '" title="填到上面的输入框">' + esc(d) + '</button>' +
				'<span class="cl-num">' + ago(it.t) + ' · ' + num(it.n) + ' 次<i class="cl-tag">' + c + '</i></span></div>' +
				(r ? '<span class="tag">' + (r.d === d ? '已设为' + ROUTE[r.r] : '按 ' + esc(r.d) + ' ' + ROUTE[r.r]) + (edit.domains ? '（未保存）' : '') + '</span>'
					: '<div class="cl-dev-r"><button type="button" class="cl-lnk" data-quick="direct" data-d="' + esc(d) + '">直连</button><button type="button" class="cl-lnk" data-quick="overseas" data-d="' + esc(d) + '">海外解析</button></div>') + '</div>';
		}
		if (!note && rec.code === 200 && !total) note = '没有包含「' + q + '」的网站。';
		if (!note && !rec.code) note = '正在读取…';
		if (note) h = '<div class="cl-dev empty">' + esc(note) + '</div>';
		if (box.getAttribute('data-h') !== h) {
			box.innerHTML = h; box.setAttribute('data-h', h);
			var b = box.getElementsByTagName('button'), j;
			for (j = 0; j < b.length; j++) {
				if (b[j].getAttribute('data-copy')) b[j].onclick = function () { var inp = $i('clDomIn'); inp.value = this.getAttribute('data-copy'); inp.focus(); try { inp.setSelectionRange(0, inp.value.length); } catch (e) {} };
				else b[j].onclick = function () { setRule(this.getAttribute('data-d'), this.getAttribute('data-quick')); };
			}
		}
		var more = total - shown;
		$i('clRecMore').hidden = !(more > 0);
		$i('clRecMore').textContent = '显示更多（还有 ' + more + ' 个）';
		$i('clRecSearch').hidden = rec.code !== 200 || !rec.items.length;
	}

	function notifyCur() { return edit.notify || { mode: db.corpline_notify || 'off', url: db.corpline_notify_url || '' }; }
	function renderNotify() {
		var n = notifyCur(), b = $i('clNtSeg').getElementsByTagName('button'), i;
		for (i = 0; i < b.length; i++) b[i].setAttribute('aria-checked', b[i].getAttribute('data-v') === n.mode ? 'true' : 'false');
		setVal('clNtVal', n.mode === 'feishu' ? '飞书' : n.mode === 'bark' ? 'Bark' : '关闭');
		$i('clNtHookWrap').className = 'cl-pend' + (n.mode === 'off' ? '' : ' on');
		var inp = $i('f-hook');
		if (!edit.notify) inp.value = n.url;
		inp.placeholder = n.mode === 'bark' ? '粘贴 Bark 的推送地址，例如 https://api.day.app/你的密钥' : '粘贴飞书群机器人的 Webhook 地址';
		var dirty = notifyDirty(), acts = $i('clNtActs'), show = dirty || n.mode !== 'off';
		acts.className = 'cl-pend' + (show ? ' on' : '');
		acts.setAttribute('aria-hidden', show ? 'false' : 'true');
		$i('clSave-notify').hidden = $i('clUndo-notify').hidden = !dirty;
		$i('clNtTest').hidden = n.mode === 'off';
		if (dirty || n.mode === 'off') ntMsg('');
	}
	function notifyDirty() {
		var n = notifyCur();
		return !!edit.notify && (n.mode !== (db.corpline_notify || 'off') || (n.mode !== 'off' && n.url !== (db.corpline_notify_url || '')));
	}
	function ntMsg(text, isErr) { var el = $i('clNtTestMsg'); el.textContent = text || ''; el.className = 'cl-msg' + (isErr ? ' err' : ''); }
	function testNotify() {
		var b = $i('clNtTest'), mode = notifyCur().mode;
		if (inflight) return;
		function send() {
			b.disabled = true; b.textContent = '正在发送…'; ntMsg('');
			submit(5, null, function (ok, res) {
				b.disabled = false; b.textContent = '发一条测试通知';
				if (!ok) ntMsg('路由器没有响应，请稍后再试。', true);
				else if (res === 'nourl') ntMsg('还没有可用的推送地址，先填好并保存。', true);
				else if (res === 'fail') ntMsg('没有发出去，检查一下推送地址是否正确。', true);
				else ntMsg(mode === 'bark' ? '已发出，看看手机有没有收到。' : '已发出，去飞书群里看看有没有收到。');
			});
		}
		// 有没保存的改动：先保存（不重连），再用保存后的设置发
		if (notifyDirty()) saveRow('notify', send); else send();
	}

	function advCur() { return edit.adv || { dns: db.corpline_isp_dns || '', sens: db.corpline_sensitivity || 'normal' }; }
	function renderAdv() {
		var a = advCur(), sel = $i('f-dns'), i, preset = 'custom';
		for (i = 0; i < ISP.length - 1; i++) if (ISP[i][0] === a.dns) preset = a.dns;
		if (!edit.adv || !edit.adv.typing) sel.value = preset;
		$i('clDnsWrap').className = 'cl-pend' + (sel.value === 'custom' ? ' on' : '');
		if (!edit.adv) $i('f-dns-c').value = preset === 'custom' ? a.dns : '';
		$i('f-sens').value = a.sens;
		setPend('adv', !!edit.adv && (a.dns !== (db.corpline_isp_dns || '') || a.sens !== (db.corpline_sensitivity || 'normal') || sel.value === 'custom' && !a.dns));
	}

	function renderToggles() {
		var t = $i('clCn');
		if (t.className.indexOf('busy') < 0) t.setAttribute('aria-checked', db.corpline_cn_direct === '1' ? 'true' : 'false');
		renderCnList();
	}

	// 中国服务名单：有多少、哪天的、正在下载、上次为什么没下成（路由器写在 corpline_cn_*）
	var cnTimer = null;
	function dayLabel(t) {
		var d = new Date(t * 1000), today = dayStart(new Date()), day = dayStart(d);
		if (day === today) return '今天';
		if (day === today - 86400000) return '昨天';
		return (d.getMonth() + 1) + '月' + d.getDate() + '日';
	}
	function thousands(n) { return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ','); }
	function cnState() {
		var s = db.corpline_cn_state || '', tried = num(db.corpline_cn_tried);
		// 下载进程被打断（路由器重启等）会停在 downloading：超过 10 分钟就当作没成功
		if (s === 'downloading' && tried && Date.now() / 1000 - tried > 600) return 'stalled';
		return s;
	}
	function renderCnList() {
		var box = $i('clCnSt'), tx = $i('clCnTx'), btn = $i('clCnUpd');
		var on = dbLoaded && db.corpline_cn_direct === '1', s = cnState();
		var n = num(db.corpline_cn_count), t = num(db.corpline_cn_time), have = n > 0;
		var cls = '', text, label = '立即更新';
		box.hidden = !on;
		clearTimeout(cnTimer); cnTimer = null;
		if (!on) return;
		if (s === 'downloading') {
			text = have ? '正在下载最新名单…（现在用的是' + (t ? dayLabel(t) : '旧') + '的名单）' : '正在下载名单…下载好会自动生效';
			label = '';
			cnTimer = setTimeout(function () { if (!document.hidden) loadDbus(renderAll); else renderCnList(); }, 3000);
		} else if (s === 'failed' || s === 'stalled') {
			var why = s === 'stalled' ? '下载被中断' : db.corpline_cn_error || '下载出错';
			var at = num(db.corpline_cn_tried);
			cls = have ? 'warn' : 'err';
			text = have ? '名单更新没成功：' + why + (at ? '（' + whenLabel(at) + '）' : '') + '，继续用' + (t ? dayLabel(t) : '旧') + '的名单'
				: '名单下载没成功：' + why + (at ? '（' + whenLabel(at) + '）' : '') + '，国内网站直连暂不生效';
			label = '重试';
		} else if (have) {
			text = '名单 ' + thousands(n) + ' 个网站 · ' + (db.corpline_cn_src === 'bundle' ? '安装包自带' + (t ? '，' + dayLabel(t) : '')
				: (t ? dayLabel(t) + '更新' : '已下载')) + ' · 每周一凌晨自动更新';
		} else {
			text = '还没有名单，国内网站直连暂不生效';
			cls = 'err';
			label = '下载';
		}
		box.className = 'cl-cnst' + (cls ? ' ' + cls : '');
		tx.textContent = text;
		btn.hidden = !label;
		btn.textContent = label;
	}

	function setPend(which, on) {
		var el = $i('clPend-' + which);
		if (!el) return;
		el.className = 'cl-pend' + (on ? ' on' : '');
		var b = el.getElementsByTagName('button'), i;
		for (i = 0; i < b.length; i++) { b[i].tabIndex = on ? 0 : -1; }
		el.setAttribute('aria-hidden', on ? 'false' : 'true');
		var hint = el.getElementsByClassName('cl-pend-hint')[0];
		if (hint) hint.textContent = db.corpline_enable === '1' ? '保存后公司专线会重新连接一次，期间家里先走直连。' : '';
	}
	function msg(which, text, isErr) {
		var el = $i('clMsg-' + which);
		if (!el) return;
		el.textContent = text || '';
		el.className = 'cl-msg' + (isErr ? ' err' : '');
	}
	function bad(inputId, which, text) {
		var el = $i(inputId);
		clearBad();
		el.className += ' bad';
		msg(which, text, true);
		el.focus();
		setTimeout(function () { el.className = el.className.replace(/\s*bad/g, ''); }, 2400);
	}
	function clearBad() {
		var all = document.querySelectorAll('.cl .cl-in.bad'), i;
		for (i = 0; i < all.length; i++) all[i].className = all[i].className.replace(/\s*bad/g, '');
	}
	function flash(text) {
		var f = $i('clFlash');
		f.textContent = text || '';
		f.className = 'cl-flash' + (text ? ' on' : '');
		if (text) setTimeout(function () { if (f.textContent === text) flash(''); }, 6000);
	}

	function renderSettings() {
		if (!dbLoaded) { renderLock(true); return; }
		renderLock(false);
		renderAccount(); renderDevices(); renderDomains(); renderNotify(); renderAdv(); renderToggles(); renderRecent();
	}
	// 还没成功读到过设置：值一律显示「—」（不是「未填写」），设置行和国内直连开关先锁住，免得用空快照保存
	var locked = null;
	function renderLock(close) {
		var on = !dbLoaded, rows = ['r-account', 'r-devices', 'r-global', 'r-notify', 'r-adv'], vals = ['clAcctVal', 'clDevVal', 'clDomVal', 'clNtVal', 'clAdvVal'], i;
		if (on) for (i = 0; i < vals.length; i++) setVal(vals[i], '—');
		else setVal('clAdvVal', '备用 DNS、判定灵敏度');
		if (locked === on && !(on && close)) return;
		locked = on;
		for (i = 0; i < rows.length; i++) {
			if (on && close && $i(rows[i]).className.indexOf(' open') >= 0) openRow(rows[i], false);
			$i(rows[i]).getElementsByClassName('cl-row-h')[0].disabled = on;
		}
		var cn = $i('clCn');
		cn.disabled = on || inflight > 0;
		if (on) cn.setAttribute('aria-checked', 'false');
	}
	function renderAll() { renderStatus(); renderTimeline(); renderSettings(); }

	/* ---------- 动作 ---------- */
	function lockedDeviceFields(over) {
		// 首次开启：记下管理电脑的 IP；能从客户端列表认出 MAC 就直接锁定加入
		if (ADMIN_IP && !db.corpline_admin_ip) over.corpline_admin_ip = ADMIN_IP;
		var list = parseDevices(db.corpline_devices), i;
		for (i = 0; i < list.length; i++) if (list[i].locked) return over;
		var me = clientByIp(ADMIN_IP);
		if (!me) return over;
		over.corpline_devices = encodeDevices([{ mac: me.mac, name: me.name, locked: true, route: 'direct', ports: '' }].concat(list));
		return over;
	}
	function onMaster() {
		var m = $i('clMaster'), key = viewKey(), was = curStarted();
		if (m.disabled || inflight) return;
		// 自动停用中点开关 = 关掉插件（同时清掉停用标记）；[0] 只关闭，不会再拉起
		var paused = key === 'paused';
		var turnOn = !paused && m.getAttribute('aria-checked') !== 'true';
		var over = paused ? { corpline_enable: '0', corpline_paused: '0' } : { corpline_enable: turnOn ? '1' : '0' };
		if (turnOn) lockedDeviceFields(over);
		m.className = 'cl-tg busy'; m.disabled = true;
		m.setAttribute('aria-checked', turnOn ? 'true' : 'false');
		$i('clSwLabel').textContent = turnOn ? '正在开启' : '正在关闭';
		submit(paused ? 0 : 1, over, function (ok) {
			m.className = 'cl-tg';
			if (ok) { flash(''); pending = turnOn ? startPending(was) : null; }
			else flash(turnOn ? '没有开启成功：路由器没有响应，请稍后再试。' : '没有关闭成功：路由器没有响应，请稍后再试。');
			renderAll();
		});
	}
	function retry() {
		if (inflight) return;
		pending = { kind: 'retry', at: Date.now() };
		shown.act = null; renderStatus();
		submit(2, null, function (ok) {
			if (!ok) { pending = null; flash('没有重试成功：路由器没有响应，请稍后再试。'); }
			setTimeout(function () { if (pending && pending.kind === 'retry') { pending = null; renderStatus(); } }, 15000);
			renderStatus();
		});
	}
	function resume() {
		if (inflight) return;
		var was = curStarted();
		pending = { kind: 'resume', at: Date.now() };
		shown.act = null; renderStatus();
		submit(3, lockedDeviceFields({ corpline_enable: '1' }), function (ok) {
			if (ok) { db.corpline_paused = '0'; pending = startPending(was); flash(''); }
			else { pending = null; flash('没有启用成功：路由器没有响应，请稍后再试。'); }
			renderAll();
		});
	}
	function restart() {
		if (inflight) return;
		var was = curStarted();
		submit(1, lockedDeviceFields({ corpline_enable: '1' }), function (ok) {
			if (ok) { pending = startPending(was); flash(''); } else flash('没有启动成功：路由器没有响应，请稍后再试。');
			renderAll();
		});
	}
	function saveRow(which, then) {
		if (inflight || !dbLoaded) return;
		var over = {}, after, was = curStarted();
		if (which === 'account') {
			var srv = trim($i('f-srv').value), user = trim($i('f-user').value), pw = $i('f-pw').value;
			if (!srv) { bad('f-srv', 'account', '请填写服务器地址。'); return; }
			if (!user) { bad('f-user', 'account', '请填写账号。'); return; }
			if (db.corpline_has_password !== '1' && !pw) { bad('f-pw', 'account', '第一次使用需要填写密码。'); return; }
			over.corpline_server = srv.replace(/^https?:\/\//, '').replace(/\/+$/, '');
			over.corpline_user = user;
			if (pw) over.corpline_password = b64e(pw);
			after = function () { $i('f-pw').value = ''; edit.account = false; openRow('r-account', false); };
		} else if (which === 'devices') {
			over.corpline_devices = encodeDevices(devList());
			after = function () { edit.devices = null; };
		} else if (which === 'domains') {
			var rl = domRules();
			over.corpline_direct_domains = rulesKey(rl, 'direct');
			over.corpline_overseas_domains = rulesKey(rl, 'overseas');
			after = function () { edit.domains = null; };
		} else if (which === 'notify') {
			var n = notifyCur(); n.url = trim($i('f-hook').value);
			if (n.mode !== 'off' && !/^https?:\/\/\S+$/.test(n.url)) { bad('f-hook', 'notify', '请粘贴完整的推送地址（以 https:// 开头）。'); return; }
			// 关掉通知时只改方式，保留已存的推送地址
			over.corpline_notify = n.mode; if (n.mode !== 'off') over.corpline_notify_url = n.url;
			after = function () { edit.notify = null; };
		} else if (which === 'adv') {
			var a = advCur(), sel = $i('f-dns').value, dns = sel === 'custom' ? trim($i('f-dns-c').value).replace(/[\s，]+/g, ',').replace(/^,|,$/g, '') : sel;
			if (sel === 'custom') {
				var parts = dns.split(','), i;
				for (i = 0; i < parts.length; i++) if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(parts[i])) { bad('f-dns-c', 'adv', '请填 IP 地址，多个用逗号隔开，例如 223.5.5.5,119.29.29.29。'); return; }
			}
			over.corpline_isp_dns = dns; over.corpline_sensitivity = $i('f-sens').value || a.sens;
			after = function () { edit.adv = null; };
		}
		// 通知设置在发送时才读取，只保存不重连；其余设置会影响线路，保存后重连
		var action = which === 'notify' ? 4 : 1;
		var btn = $i('clSave-' + which);
		btn.disabled = true; msg(which, '正在保存…');
		submit(action, over, function (ok) {
			btn.disabled = false;
			if (!ok) { msg(which, failText(), true); return; }
			msg(which, '');
			clearBad();
			after();
			if (action === 1 && db.corpline_enable === '1' && db.corpline_paused !== '1') pending = startPending(was);
			renderAll();
			if (then) then();
		});
	}
	function undoRow(which) {
		edit[which] = which === 'account' ? false : null;
		msg(which, '');
		if (which === 'devices') { $i('clDevMac').value = ''; $i('clDevName').value = ''; portsOpen = ''; }
		if (which === 'domains') $i('clDomIn').value = '';
		renderSettings();
	}
	function onCn() {
		var t = $i('clCn'), want = t.getAttribute('aria-checked') !== 'true', was = curStarted();
		if (t.disabled || inflight || !dbLoaded) return;
		t.setAttribute('aria-checked', want ? 'true' : 'false');
		t.className = 'cl-tg busy'; t.disabled = true;
		submit(1, { corpline_cn_direct: want ? '1' : '0' }, function (ok) {
			t.className = 'cl-tg'; t.disabled = false;
			if (!ok) { t.setAttribute('aria-checked', want ? 'false' : 'true'); flash(failText()); }
			else if (db.corpline_enable === '1' && db.corpline_paused !== '1') pending = startPending(was);
			renderAll();
		});
	}

	function onCnUpd() {
		if (inflight || !dbLoaded) return;
		submit(6, null, function (ok) {
			if (!ok) { flash(failText()); return; }
			db.corpline_cn_state = 'downloading';
			db.corpline_cn_tried = String(Math.floor(Date.now() / 1000));
			renderCnList();
		});
	}

	/* ---------- 行展开 ---------- */
	function openRow(id, force, quiet) {
		var r = $i(id), open = force === undefined ? r.className.indexOf(' open') < 0 : force;
		r.className = r.className.replace(/\s*open/g, '') + (open ? ' open' : '');
		var h = r.getElementsByClassName('cl-row-h')[0];
		h.setAttribute('aria-expanded', open ? 'true' : 'false');
		var p = r.getElementsByTagName('path')[0];
		if (p) p.setAttribute('d', open ? 'M4 6l4 4 4-4' : 'M6 4l4 4-4 4');
		var inner = r.getElementsByClassName('cl-body')[0];
		if (inner) inner.setAttribute('aria-hidden', open ? 'false' : 'true');
		if (id === 'r-global') syncRecent();
		if (open && force && !quiet && r.scrollIntoView) {
			var reduce = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;
			setTimeout(function () { try { r.scrollIntoView({ behavior: reduce ? 'auto' : 'smooth', block: 'center' }); } catch (e) { r.scrollIntoView(); } }, 60);
		}
	}
	function focusLater(id) { setTimeout(function () { var el = $i(id); if (el) try { el.focus({ preventScroll: true }); } catch (e) { el.focus(); } }, 260); }

	/* ---------- 运行日志 ---------- */
	function loadLog() {
		xhr('GET', '/_temp/corpline_log.txt?_=' + Date.now(), null, function (code, text) {
			var box = $i('clLog');
			var atBottom = box.scrollTop + box.clientHeight >= box.scrollHeight - 24;
			var t = code >= 200 && code < 300 && trim(text) ? text : '还没有运行日志。';
			var lines = t.replace(/\s+$/, '').split('\n');
			if (lines.length > 400) lines = lines.slice(lines.length - 400);
			t = lines.join('\n');
			if (box.textContent !== t) { box.textContent = t; if (atBottom || !box.getAttribute('data-seen')) box.scrollTop = box.scrollHeight; box.setAttribute('data-seen', '1'); }
		});
	}
	function toggleLog() {
		logOpen = !logOpen;
		$i('clLogWrap').className = 'cl-xp' + (logOpen ? ' on' : '');
		$i('clLogWrap').setAttribute('aria-hidden', logOpen ? 'false' : 'true');
		$i('clLogBtn').textContent = logOpen ? '收起运行日志' : '查看运行日志';
		$i('clLogBtn').setAttribute('aria-expanded', logOpen ? 'true' : 'false');
		clearInterval(logTimer);
		if (logOpen) { $i('clLog').removeAttribute('data-seen'); loadLog(); logTimer = setInterval(loadLog, POLL_MS); }
	}

	/* ---------- 轮询 ---------- */
	function poll(done) {
		getJSON('/_temp/corpline_status.json', function (code, o) {
			if (o && typeof o === 'object' && (o.state || o.v)) {
				reach(true);
				// 比手上这份还旧的回复不要（同一个进程的 updated 只会往前走）
				var older = st && num(o.updated) < num(st.updated) && o.started === st.started;
				if (!older) {
					if (prevUpd && o.updated && o.updated !== prevUpd) updMovedAt = Date.now();
					prevUpd = o.updated || 0;
					st = o; stAt = Date.now();
					if (pending && pending.kind === 'retry' && (o.retry && o.retry.connecting || o.state !== 'fallback')) pending = null;
				}
			} else if (code === 404) { reach(true); st = null; }   // 路由器在，只是还没有状态文件
			else reach(false);
			waitPoll = false;
			renderStatus(); renderTimeline();
			done();
		});
		polls++;
		if (polls % 3 === 0 || !dbLoaded) loadDbus(function () { renderStatus(); renderSettings(); });
	}
	// 同一时间只有一个读取在路上，读完再排下一次：平时每 5 秒；刚操作过的 30 秒内每 1.5 秒，让状态变化跟得上
	function loop() {
		clearTimeout(pollTimer); pollTimer = null;
		if (polling || document.hidden) return;
		polling = true;
		poll(function () {
			polling = false;
			var ms = pollSoon || (Date.now() < fastUntil ? 1500 : POLL_MS);
			pollSoon = 0;
			pollIn(ms);
		});
	}
	function pollIn(ms) {
		if (polling) { pollSoon = pollSoon ? Math.min(pollSoon, ms) : ms; return; }
		clearTimeout(pollTimer); pollTimer = null;
		if (!document.hidden) pollTimer = setTimeout(loop, ms);
	}
	// 页面在后台时不读；切回来马上读一次，读到之前先不刷新状态卡，免得闪一下「没有在运行」
	function onVisible() {
		syncRecent();
		if (document.hidden) { clearTimeout(pollTimer); pollTimer = null; return; }
		if (!polling) { waitPoll = true; loop(); }
	}

	/* ---------- 绑定 ---------- */
	function bind() {
		$i('clMaster').onclick = onMaster;
		$i('clCn').onclick = onCn;
		$i('clCnUpd').onclick = onCnUpd;
		$i('clLogBtn').onclick = toggleLog;
		var heads = document.querySelectorAll('.cl .cl-row:not(.static) > .cl-row-h'), i;
		for (i = 0; i < heads.length; i++) heads[i].onclick = function () { openRow(this.parentNode.id); };
		$i('clAcctCancel').onclick = function () { $i('f-pw').value = ''; undoRow('account'); openRow('r-account', false); };
		var ids = ['f-srv', 'f-user', 'f-pw'];
		for (i = 0; i < ids.length; i++) $i(ids[i]).oninput = function () { edit.account = true; if ($i('clMsg-account').className.indexOf('err') >= 0) msg('account', ''); };
		var saves = ['account', 'devices', 'domains', 'notify', 'adv'];
		for (i = 0; i < saves.length; i++) (function (w) {
			$i('clSave-' + w).onclick = function () { saveRow(w); };
			var u = $i('clUndo-' + w); if (u) u.onclick = function () { undoRow(w); };
		})(saves[i]);
		$i('clDevAddM').onclick = addDevice;
		$i('clDevManualBtn').onclick = function () { devManual = true; renderDevices(); focusLater('clDevName'); };
		$i('clDomAdd').onclick = addDomain;
		var as = $i('clDomAddSeg').getElementsByTagName('button');
		for (i = 0; i < as.length; i++) as[i].onclick = function () { addRoute = this.getAttribute('data-v'); renderDomains(); };
		$i('clRecQ').oninput = function () { recLimit = 30; renderRecent(); };
		$i('clRecMore').onclick = function () { recLimit += 30; renderRecent(); };
		$i('clNtTest').onclick = testNotify;
		$i('clDomIn').onkeydown = function (e) { if ((e || window.event).keyCode === 13) addDomain(); };
		$i('clDevMac').onkeydown = function (e) { if ((e || window.event).keyCode === 13) addDevice(); };
		var segs = $i('clNtSeg').getElementsByTagName('button');
		for (i = 0; i < segs.length; i++) segs[i].onclick = function () {
			var n = notifyCur(); edit.notify = { mode: this.getAttribute('data-v'), url: trim($i('f-hook').value) || n.url };
			msg('notify', ''); renderNotify();
			if (edit.notify.mode !== 'off' && !edit.notify.url) focusLater('f-hook');
		};
		$i('f-hook').oninput = function () { var n = notifyCur(); edit.notify = { mode: n.mode, url: trim(this.value) }; renderNotify(); };
		$i('f-dns').onchange = function () {
			var a = advCur(); edit.adv = { dns: this.value === 'custom' ? trim($i('f-dns-c').value) : this.value, sens: a.sens, typing: true };
			renderAdv(); if (this.value === 'custom') focusLater('f-dns-c');
		};
		$i('f-dns-c').oninput = function () { var a = advCur(); edit.adv = { dns: trim(this.value), sens: a.sens, typing: true }; renderAdv(); };
		$i('f-sens').onchange = function () { var a = advCur(); edit.adv = { dns: a.dns, sens: this.value, typing: a.typing }; renderAdv(); };
	}

	function start() {
		bind();
		renderStatus(); renderTimeline(); renderLock(false);
		loadDbus(function () { renderAll(); loop(); });
		loadClients();
		if (document.addEventListener) document.addEventListener('visibilitychange', onVisible, false);
		setInterval(function () { if (!document.hidden && !waitPoll && (st || pending || net.fails)) renderStatus(); }, 1000);
	}

	return { start: start, openRow: openRow };
})();

function init() {
	if (typeof show_menu === 'function') show_menu(menu_hook);
	CL.start();
}
function menu_hook() {
	tabtitle[tabtitle.length - 1] = new Array("", "公司专线");
	tablink[tablink.length - 1] = new Array("", "Module_corpline.asp");
}
</script>
</head>
<body onload="init();" class="bg">
<div id="TopBanner"></div>
<div id="Loading" class="popup_bg"></div>
<table class="content" align="center" cellpadding="0" cellspacing="0"><tr>
<td width="17">&nbsp;</td>
<td valign="top" width="202"><div id="mainMenu"></div><div id="subMenu"></div></td>
<td valign="top">
<div id="tabMenu" class="submenuBlock"></div>
<table width="98%" border="0" align="left" cellpadding="0" cellspacing="0"><tr><td valign="top">
<table width="760px" border="0" cellpadding="0" cellspacing="0" class="FormTitle" id="FormTitle"><tr><td bgcolor="#4D595D" valign="top">

<div class="cl" id="cl">
  <div class="cl-crumb"><span>软件中心 › 公司专线</span><a href="/Module_Softcenter.asp">返回软件中心</a></div>

  <div class="cl-head">
    <div class="cl-tt">
      <h1 class="cl-h1">公司专线</h1>
      <p class="cl-sub">全家上网走公司线路。线路不稳时自动切回直连，恢复后自动切回。</p>
    </div>
    <div class="cl-sw"><span id="clSwLabel"></span><button type="button" class="cl-tg" id="clMaster" role="switch" aria-checked="false" aria-label="公司专线总开关" disabled></button></div>
  </div>
  <p class="cl-flash" id="clFlash" role="alert"></p>

  <section class="cl-card cl-status" aria-live="polite">
    <div class="cl-st-body" id="clStBody">
      <div class="cl-st-top"><span class="cl-dot idle" id="clStDot"></span><h2 class="cl-st-title" id="clStTitle">正在读取状态…</h2><span id="clStAct"></span></div>
      <p class="cl-st-desc" id="clStDesc" hidden></p>
      <div class="cl-metrics" id="clMetrics"></div>
    </div>
  </section>

  <div class="cl-sec-h"><h2>最近发生了什么</h2><button type="button" class="cl-lnk" id="clLogBtn" aria-expanded="false" aria-controls="clLogWrap">查看运行日志</button></div>
  <ul class="cl-card cl-tl" id="clTimeline"></ul>
  <div class="cl-xp" id="clLogWrap" aria-hidden="true"><div><div class="cl-log" id="clLog" tabindex="0" aria-label="运行日志"></div></div></div>

  <div class="cl-sec-h"><h2>设置</h2></div>
  <div class="cl-card" id="clRows">

    <div class="cl-row" id="r-account">
      <button type="button" class="cl-row-h" aria-expanded="false"><span class="lb"><b>公司账号</b></span><span class="val" id="clAcctVal"></span><svg class="cl-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg></button>
      <div class="cl-body" aria-hidden="true"><div><div class="cl-xp-in">
        <div class="cl-fg">
          <label for="f-srv">服务器</label><input class="cl-in" id="f-srv" autocomplete="off" spellcheck="false">
          <label for="f-user">账号</label><input class="cl-in" id="f-user" autocomplete="username" spellcheck="false">
          <label for="f-pw">密码</label><input class="cl-in" id="f-pw" type="password" autocomplete="new-password">
        </div>
        <p class="cl-hint" id="clAcctHint"></p>
        <div class="cl-acts"><button type="button" class="cl-btn pri" id="clSave-account">保存</button><button type="button" class="cl-btn sec" id="clAcctCancel">取消</button><span class="cl-msg" id="clMsg-account" role="status"></span></div>
      </div></div></div>
    </div>

    <div class="cl-row" id="r-devices">
      <button type="button" class="cl-row-h" aria-expanded="false"><span class="lb"><b>家里的设备</b><span>每台设备走哪条线</span></span><span class="val cl-num" id="clDevVal"></span><svg class="cl-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg></button>
      <div class="cl-body" aria-hidden="true"><div><div class="cl-xp-in">
        <div id="clDevList"></div>
        <button type="button" class="cl-lnk cl-more" id="clDevManualBtn">手动添加不在线的设备…</button>
        <div class="cl-add" id="clDevManual" hidden><input class="cl-in" id="clDevName" placeholder="名字，例如 NAS" aria-label="设备名字"><input class="cl-in cl-num" id="clDevMac" placeholder="MAC 地址" aria-label="MAC 地址" spellcheck="false"><button type="button" class="cl-btn sec" id="clDevAddM">设为直连</button></div>
        <p class="cl-msg err cl-rowmsg" id="clMsg-devices" role="status"></p>
        <p class="cl-hint" id="clDevNote" hidden></p>
        <p class="cl-hint">直连的设备不走公司线路，直接用家里宽带上网。</p>
        <div class="cl-pend" id="clPend-devices" aria-hidden="true"><div><div class="cl-acts"><button type="button" class="cl-btn pri" id="clSave-devices" tabindex="-1">保存</button><button type="button" class="cl-btn sec" id="clUndo-devices" tabindex="-1">撤销</button><span class="cl-msg cl-pend-hint"></span></div></div></div>
      </div></div></div>
    </div>

    <div class="cl-row" id="r-global">
      <button type="button" class="cl-row-h" aria-expanded="false"><span class="lb"><b>网站单独设置</b><span>个别网站改走直连，或改用海外解析</span></span><span class="val cl-num" id="clDomVal"></span><svg class="cl-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg></button>
      <div class="cl-body" aria-hidden="true"><div><div class="cl-xp-in">
        <div class="cl-subh">已设置</div>
        <div id="clDomList"></div>
        <div class="cl-add"><input class="cl-in" id="clDomIn" placeholder="添加网址，例如 nintendo.co.jp" aria-label="添加网址" spellcheck="false"><div class="cl-seg q" id="clDomAddSeg" role="radiogroup" aria-label="添加为"><button type="button" role="radio" aria-checked="true" data-v="direct">直连</button><button type="button" role="radio" aria-checked="false" data-v="overseas">海外解析</button></div><button type="button" class="cl-btn sec" id="clDomAdd">添加</button></div>
        <p class="cl-msg err cl-rowmsg" id="clMsg-domains" role="status"></p>
        <p class="cl-hint">直连：不走公司线路，直接用家里宽带。海外解析：还走公司线路，只是改用海外节点，适合被分到国内节点的海外网站。</p>
        <div class="cl-pend" id="clPend-domains" aria-hidden="true"><div><div class="cl-acts"><button type="button" class="cl-btn pri" id="clSave-domains" tabindex="-1">保存</button><button type="button" class="cl-btn sec" id="clUndo-domains" tabindex="-1">撤销</button><span class="cl-msg cl-pend-hint"></span></div></div></div>
        <div class="cl-subh">最近访问</div>
        <div class="cl-add" id="clRecSearch" hidden><input class="cl-in" id="clRecQ" type="search" placeholder="搜索网址，例如 qq" aria-label="搜索最近访问的网站" spellcheck="false" autocomplete="off"></div>
        <div id="clRecList"></div>
        <button type="button" class="cl-lnk cl-more" id="clRecMore" hidden></button>
        <p class="cl-hint">规则对子域名同样生效；想放宽范围，在上面输入上一级域名，例如 qq.com。</p>
      </div></div></div>
    </div>

    <div class="cl-row static" id="r-cn">
      <div class="cl-row-h"><span class="lb"><b>国内网站直连</b><span>淘宝、B 站、微信这类国内服务不经过公司线路，用满家里宽带。Apple、Sony 等海外服务不受影响。</span></span><button type="button" class="cl-tg" id="clCn" role="switch" aria-checked="false" aria-label="国内网站直连"></button></div>
      <div class="cl-cnst" id="clCnSt" hidden><span class="tx" id="clCnTx" role="status"></span><button type="button" class="cl-lnk" id="clCnUpd">立即更新</button></div>
    </div>

    <div class="cl-row" id="r-notify">
      <button type="button" class="cl-row-h" aria-expanded="false"><span class="lb"><b>异常通知</b><span>直连超过 30 分钟或账号登录失败时提醒你</span></span><span class="val" id="clNtVal"></span><svg class="cl-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg></button>
      <div class="cl-body" aria-hidden="true"><div><div class="cl-xp-in">
        <div class="cl-seg" id="clNtSeg" role="radiogroup" aria-label="通知方式"><button type="button" role="radio" data-v="off">关闭</button><button type="button" role="radio" data-v="feishu">飞书</button><button type="button" role="radio" data-v="bark">Bark</button></div>
        <div class="cl-pend" id="clNtHookWrap"><div><div class="cl-fg" style="margin-top:16px"><label for="f-hook">推送地址</label><input class="cl-in" id="f-hook" spellcheck="false"></div></div></div>
        <p class="cl-msg err cl-rowmsg" id="clMsg-notify" role="status"></p>
        <div class="cl-pend" id="clNtActs" aria-hidden="true"><div><div class="cl-acts"><button type="button" class="cl-btn pri" id="clSave-notify">保存</button><button type="button" class="cl-btn sec" id="clUndo-notify">撤销</button><button type="button" class="cl-btn sec" id="clNtTest">发一条测试通知</button><span class="cl-msg" id="clNtTestMsg" role="status"></span></div></div></div>
      </div></div></div>
    </div>

    <div class="cl-row" id="r-adv">
      <button type="button" class="cl-row-h" aria-expanded="false"><span class="lb"><b>高级</b></span><span class="val" id="clAdvVal"></span><svg class="cl-chev" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l4 4-4 4"/></svg></button>
      <div class="cl-body" aria-hidden="true"><div><div class="cl-xp-in">
        <div class="cl-fg">
          <label for="f-dns">备用 DNS</label><select class="cl-in" id="f-dns"><option value="">运营商 DNS（自动）</option><option value="223.5.5.5,223.6.6.6">阿里 223.5.5.5</option><option value="119.29.29.29,119.28.28.28">腾讯 119.29.29.29</option><option value="custom">自定义…</option></select>
        </div>
        <div class="cl-pend" id="clDnsWrap"><div><div class="cl-fg" style="margin-top:12px"><label for="f-dns-c">DNS 地址</label><input class="cl-in cl-num" id="f-dns-c" placeholder="例如 223.5.5.5,119.29.29.29" spellcheck="false"></div></div></div>
        <div class="cl-fg" style="margin-top:12px">
          <label for="f-sens">判定灵敏度</label><select class="cl-in" id="f-sens"><option value="normal">普通：约 45 秒没有响应才切到直连</option><option value="relaxed">宽松：约 75 秒没有响应才切到直连</option></select>
        </div>
        <p class="cl-msg err cl-rowmsg" id="clMsg-adv" role="status"></p>
        <p class="cl-hint">公司 DNS 没响应时自动改用备用 DNS，恢复后自动切回。判定越宽松，越不容易误切，但出问题时等得更久。</p>
        <div class="cl-pend" id="clPend-adv" aria-hidden="true"><div><div class="cl-acts"><button type="button" class="cl-btn pri" id="clSave-adv" tabindex="-1">保存</button><button type="button" class="cl-btn sec" id="clUndo-adv" tabindex="-1">撤销</button><span class="cl-msg cl-pend-hint"></span></div></div></div>
      </div></div></div>
    </div>
  </div>

  <div class="cl-foot"><span class="cl-num" id="clFoot"></span></div>
</div>

</td></tr></table>
</td></tr></table>
</td>
<td width="10"></td>
</tr></table>
<div id="footer"></div>
</body>
</html>
