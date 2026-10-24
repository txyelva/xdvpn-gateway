/* 本机预览用的 show_menu 替身：画出与原型一致的华硕外框 */
var tabtitle = [[]], tablink = [[]];
function show_menu(hook) {
	if (hook) hook();
	var b = document.getElementById('TopBanner');
	if (b) b.innerHTML = '<div class="stub-banner"><b>ASUS</b><span>RT-AX86U</span><i></i><span>固件 388_24231 · koolcenter</span></div>';
	var m = document.getElementById('mainMenu');
	if (m) m.innerHTML = '<div class="m">网络地图</div><div class="m">访客网络</div><div class="m">流量分析</div><div class="m on">软件中心</div>' +
		'<div class="cap">高级设置</div><div class="m">无线网络</div><div class="m">内部网络</div><div class="m">外部网络</div><div class="m">防火墙</div><div class="m">系统管理</div><div class="m">系统记录</div>';
}
