# iOS 代理客户端获取 Garupa UID 与 UUID

这套脚本只用于自己的 BanG Dream! Garupa 账号调试。它只读取匹配请求的
`/api/user/<UID>` URL 和 `X-Signature` 请求头，不修改游戏请求，也不上传请求体、
加密 Key 或 IV。

## 支持的客户端

| 客户端 | 配置文件 | 脚本文件 | 额外说明 |
| --- | --- | --- | --- |
| Loon | [`penlight-credentials.plugin`](../ios/loon/penlight-credentials.plugin) | [`penlight-credentials.js`](../ios/loon/penlight-credentials.js) | 支持通知复制和手动显示 |
| Shadowrocket | [`penlight-credentials.module`](../ios/shadowrocket/penlight-credentials.module) | 远程自动加载 | 导入模块后自动捕获 |
| Surge | [`penlight-credentials.sgmodule`](../ios/surge/penlight-credentials.sgmodule) | 远程自动加载 | 可手动运行显示脚本 |
| Stash | [`penlight-credentials.stoverride`](../ios/stash/penlight-credentials.stoverride) | 远程自动加载 | 主页 Tile 显示已保存内容 |
| Quantumult X | [`penlight-credentials.conf`](../ios/quantumultx/penlight-credentials.conf) | [`penlight-credentials.js`](../ios/quantumultx/penlight-credentials.js) | 脚本需要保存到本地 Scripts 文件夹 |

Shadowrocket、Surge 和 Stash 会从 [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js)
加载通用脚本；Loon 保留现有的本地插件脚本，Quantumult X 使用它自己的本地脚本适配。


```text
https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/shadowrocket/penlight-credentials.module
https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/surge/penlight-credentials.sgmodule
https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/loon/penlight-credentials.plugin
```

Stash 可导入 [`penlight-credentials.stoverride`](../ios/stash/penlight-credentials.stoverride)，
Quantumult X 需要按下方说明把 JS 保存到本地。

## 使用步骤

1. 在对应客户端导入上表中的配置文件；Quantumult X 先把它的 JS 文件保存到
   “我的 iPhone / Quantumult X / Scripts / penlight-credentials.js”。
2. 为 `api.garupa.jp` 开启 MITM，安装并信任客户端证书。
3. 启用配置，打开游戏并进入个人资料或其他会访问 Garupa 用户接口的页面。
4. 收到通知后点击复制 JSON。Surge 可手动运行 `penlight-credentials-show`；Loon
   可手动运行“显示已保存 UID UUID”；Stash 可在主页 Tile 查看结果。

Quantumult X 使用通知的 `update-pasteboard` 功能复制 JSON；如果没有弹出通知，
可再次打开游戏触发一次匹配请求。

复制出的结果形如：

```json
{
  "uid": "123456789",
  "uuid": "设备请求中的 X-Signature 值"
}
```

## 与服务端配置的关系

本项目把 `GARUPA_UUIDS` 定义为请求头 `X-Signature`，不是响应 protobuf 中另一个名为
`uuid` 的字段。旧的固定玩家采集流程可以把结果填入：

```dotenv
GARUPA_UIDS=123456789
GARUPA_UUIDS=设备请求中的 X-Signature 值
```

网页的动态 POST 导出接口不要求每次修改这两个值；只需在服务端预先配置解密 Key/IV。

不要把完整通知、代理客户端本地存储或 `.env` 上传到公共仓库；UID/UUID 应仅用于自己的账号。
