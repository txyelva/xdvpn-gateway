# xdvpn-gateway（公司专线）

[XDVPN](https://github.com/zqchris/XDVPN) 的路由器版本。华硕 RT-AX86U（koolcenter 软件中心，hnd 平台）插件：路由器替全家拨通公司 OpenConnect 专线，任何环节出问题自动回到运营商直连。取代旧版 [xdvpn-router](https://github.com/zqchris/xdvpn-router)。

- 设计：[docs/DESIGN.md](docs/DESIGN.md)
- 沙盒演练结果：[docs/LAB-RESULTS.md](docs/LAB-RESULTS.md)
- 状态：0.1.0，只在本机沙盒测过（19 个故障场景、93 项检查全部通过），**尚未安装到任何路由器**。

## 目录

| 路径 | 内容 |
|---|---|
| `corpline/` | 守护程序（Rust，只依赖 libc，静态 aarch64 约 0.65 MB）：状态机、DNS 转发器、路由/防火墙、openconnect 管理 |
| `plugin/corpline/` | 软件中心插件：安装/卸载脚本、外壳脚本 `corpline_config.sh`、网页 `Module_corpline.asp`、图标 |
| `lab/` | 沙盒：Linux 虚拟机里用网络命名空间模拟家庭网络、运营商、公司 VPN（ocserv）与软件中心；`run.sh` 逐项故障演练 |
| `lab/web/` | 网页的本地预览与交互检查（模拟 dbus / 状态文件） |
| `docs/` | 设计文档与演练结果 |

## 构建

在装有 Rust 与 `aarch64-unknown-linux-musl` 目标的 Linux（例如沙盒虚拟机）上：

```sh
bash build.sh        # 生成 corpline.tar.gz（软件中心离线安装包）
```

## 沙盒测试

```sh
sudo bash lab/setup.sh             # 搭沙盒网络（只在虚拟机的网络命名空间里）
sudo bash lab/run.sh               # 全部场景，约 45 分钟；结果写入 lab/report/RESULTS.md
sudo bash lab/run.sh crash notify  # 只跑指定场景
sudo bash lab/mem_cn.sh <名单>     # 用真实国内服务名单测内存与解析
sudo bash lab/teardown.sh          # 拆掉沙盒
```

## 安装须知

安装后默认**不启用、不改任何网络设置**；在插件页填好公司账号再打开开关。需要 Entware 的 `openconnect`（`opkg install openconnect`）。关闭或卸载会撤掉全部改动，恢复到安装前。
