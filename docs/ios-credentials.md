# iOS 代理客户端获取 Garupa UID 与 UUID

这套脚本只用于自己的 BanG Dream! Garupa 账号调试。它只读取匹配请求的
`/api/user/<UID>` URL 和 `X-Signature` 请求头，不修改游戏请求，也不上传请求体、
加密 Key 或 IV。

## 支持的客户端

| 客户端 | 远程导入文件 | 远程脚本 | 额外说明 |
| --- | --- | --- | --- |
| Loon | [`penlight-credentials.plugin`](../ios/loon/penlight-credentials.plugin) | [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js) | 远程导入插件，支持通知复制和手动显示 |
| Shadowrocket | [`penlight-credentials.module`](../ios/shadowrocket/penlight-credentials.module) | [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js) | 远程导入模块后自动捕获 |
| Surge | [`penlight-credentials.sgmodule`](../ios/surge/penlight-credentials.sgmodule) | [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js) | 远程导入模块，可手动运行显示脚本 |
| Stash | [`penlight-credentials.stoverride`](../ios/stash/penlight-credentials.stoverride) | [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js) | 远程导入 Override，主页 Tile 显示已保存内容 |
| Quantumult X | [`penlight-credentials.remote.snippet`](../ios/quantumultx/penlight-credentials.remote.snippet) | [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js) | 通过网页一键添加远程片段 |

所有客户端均从 [`ios/common/penlight-credentials.js`](../ios/common/penlight-credentials.js)
远程加载脚本，不再需要把 JS 文件保存到设备。

## 安装

请手动复制对应地址，粘贴到客户端的远程导入或添加页面。Loon 导入的是 Plugin，Stash 导入的是 Override，Quantumult X 添加的是远程 Rewrite 片段。

| 软件 | raw GitHub 链接 |
| --- | --- |
| Shadowrocket | https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/shadowrocket/penlight-credentials.module |
| Surge | https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/surge/penlight-credentials.sgmodule |
| Stash | https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/stash/penlight-credentials.stoverride |
| Loon | https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/loon/penlight-credentials.plugin |
| Quantumult X | https://raw.githubusercontent.com/AsterZC19/Penlight-Dream-Box/main/ios/quantumultx/penlight-credentials.remote.snippet |


## 使用步骤

1. 在网页首页“**一键安装抓取配置**”中点击对应客户端，直接打开导入页面。
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
