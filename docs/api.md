# Penlight-Dream-Box API 文档


## 基础信息

| 项 | 值 |
|---|---|
| 默认端口 | 8081，裸部署由 `BOX_PORT` 指定 |
| API 前缀 | `/api`，由 `BOX_API_PREFIX` 指定 |
| 响应格式 | JSON，UTF-8 |
| 响应压缩 | 支持 gzip / br，请求带 `Accept-Encoding: gzip` 自动启用 |
| 认证 | 可选。设置 `API_KEY` 后所有 `/api/*` 请求须携带 `X-API-Key: <key>` 或 `Authorization: Bearer <key>` |

## 通用约定

### 参数传递

全部端点使用 GET 方法，参数通过 URL 查询字符串传递，形如 `?server=0&monthlyId=2&tier=100`。

### server 参数

| 值 | 含义 |
|---|---|
| `0` | 日服，全部接口接受 |
| `jp` | 日服，仅 `eventtop/data` 与 `tracker/data` 接受 |

本服务只采集日服数据，其他值返回 422。

### 时间

全部时间戳为 Unix 毫秒。`time`、`startAt`、`endAt`、`since` 均为此单位。

### 错误格式

参数校验失败：

```json
{
  "status": 422,
  "message": "Validation Failed",
  "details": [
    {
      "message": "tier must be one of: 20,30,40,50,100,200,300,500,1000,2000,3000,4000,5000",
      "code": "invalid",
      "field": "tier"
    }
  ]
}
```

其他错误：

```json
{
  "status": 500,
  "message": "Internal Server Error"
}
```

### 空数据语义

| 接口 | 无数据时的行为 |
|---|---|
| `monthlyRanking/top` | 返回 `{"points": [], "users": []}` |
| `monthlyRanking/border` | 返回 `{"result": true, "cutoffs": []}` |
| `eventtop/data` | 302 重定向到 `https://bestdori.com{原路径}` |
| `tracker/data` | 302 重定向到 `https://bestdori.com{原路径}` |

### 数据字段

`points` 数组元素：

```json
{
  "time": 1786844093675,
  "uid": 1001,
  "value": 28501234
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `time` | number | 快照时间，Unix 毫秒 |
| `uid` | number | 玩家 UID |
| `value` | number | 当期 PT |

`users` 数组元素：

```json
{
  "uid": 1001,
  "name": "ひまり",
  "introduction": "よろしくお願いします！",
  "rank": 99,
  "sid": 7,
  "strained": 0,
  "degrees": [1]
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `uid` | number | 玩家 UID |
| `name` | string | 玩家名 |
| `introduction` | string | 签名 |
| `rank` | number | 玩家等级 |
| `sid` | number | 展示卡片 ID |
| `strained` | number | 是否为觉醒立绘，1 是 0 否 |
| `degrees` | number[] | 玩家称号 ID 列表 |

`cutoffs` 数组元素：

```json
{
  "time": 1786844093666,
  "ep": 14500000
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `time` | number | 快照时间，Unix 毫秒 |
| `ep` | number | 该档位分数线 |

### 档位 tier 白名单

| 范围 | 合法值 |
|---|---|
| 月榜 | 20, 30, 40, 50, 100, 200, 300, 500, 1000, 2000, 3000, 4000, 5000 |
| 活动榜 | 20, 30, 40, 50, 100, 200, 300, 500, 1000, 1500, 2000, 3000, 4000, 5000, 10000, 20000, 30000, 40000, 50000, 100000 |
| 歌榜 | 20, 30, 40, 50, 100, 200, 300, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000 |

## 端点

### GET /health

健康检查，位于 API 前缀之外。

响应：

```json
{
  "status": "ok"
}
```

### GET /api/monthlyRanking/info 与 /api/monthlyRanking/info.json

全部月榜期列表，轻量视图。

请求：

```
GET /api/monthlyRanking/info.json
curl http://127.0.0.1:8081/api/monthlyRanking/info.json
```

无参数。

响应：对象，键为月榜 ID 字符串。

```json
{
  "1": {
    "monthlyRankingName": ["バンドリ！月間ランキング 2025/07", null, null, null, null],
    "assetBundleName": "monthly_202507",
    "bgmFileName": "bgm_rank",
    "startAt": [1785116007589, null, null, null, null],
    "endAt": [1785980007589, null, null, null, null]
  }
}
```

| 字段 | 类型 | 说明 |
|---|---|---|
| `monthlyRankingName` | string[] | 月榜名称，5 元素服务器数组，index 0 为日服 |
| `assetBundleName` | string | 素材包名，可拼 Bestdori 头图地址 |
| `bgmFileName` | string | BGM 文件名 |
| `startAt` | number[] | 开始时间，5 元素服务器数组 |
| `endAt` | number[] | 结束时间，0 表示未提供结束时间，视为进行中 |

### GET /api/monthlyRanking/all 与 /api/monthlyRanking/all.json

全部月榜期完整信息，包含 rewards 与 grades。

请求：

```
GET /api/monthlyRanking/all.json
curl http://127.0.0.1:8081/api/monthlyRanking/all.json
```

无参数。

响应结构同 info，增加 `enableFlag`、`publicStartAt`、`publicEndAt`、`distributionStartAt`、`distributionEndAt`、`aggregateEndAt`、`receptionEndAt`、`rewards`、`grades` 字段，均为 5 元素服务器数组或原始数组。

### GET /api/monthlyRanking/top 与 /api/monthlyRanking/top.json

某期月榜的 top 快照历史。

| 参数 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `server` | 是 | 无 | 服务器，仅 `0` |
| `monthlyId` | 否 | 当前进行中的月榜 | 月榜 ID，>= 1 |
| `interval` | 否 | 60000 | 输出重采样间隔，毫秒，>= 1 |
| `since` | 否 | 0 | 只返回该时间之后的点，毫秒，>= 0 |

请求：

```
GET /api/monthlyRanking/top?server=0&monthlyId=2
GET /api/monthlyRanking/top?server=0&monthlyId=2&interval=900000&since=1786844000000
curl "http://127.0.0.1:8081/api/monthlyRanking/top?server=0&monthlyId=2"
```

响应：

```json
{
  "points": [
    { "time": 1786844093675, "uid": 1001, "value": 28501234 }
  ],
  "users": [
    {
      "uid": 1001,
      "name": "ひまり",
      "introduction": "よろしくお願いします！",
      "rank": 99,
      "sid": 7,
      "strained": 0,
      "degrees": [1]
    }
  ]
}
```

`since` 在存储层按天分桶过滤，只读取相关文档。`interval` 大于 60000 时按 uid 与时间桶取每桶最后一点。

### GET /api/monthlyRanking/border 与 /api/monthlyRanking/border.json

某期月榜指定档位的分数线历史。

| 参数 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `server` | 是 | 无 | 服务器，仅 `0` |
| `monthlyId` | 否 | 当前进行中的月榜 | 月榜 ID，>= 1 |
| `tier` | 是 | 无 | 档位，须在月榜白名单内 |

请求：

```
GET /api/monthlyRanking/border?server=0&tier=100
GET /api/monthlyRanking/border?server=0&monthlyId=2&tier=100
curl "http://127.0.0.1:8081/api/monthlyRanking/border?server=0&monthlyId=2&tier=100"
```

响应：

```json
{
  "result": true,
  "cutoffs": [
    { "time": 1786844093666, "ep": 14500000 }
  ]
}
```

### GET /api/eventtop/data

活动榜或歌榜的 top 快照历史。

| 参数 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `server` | 是 | 无 | 服务器，`0` 或 `jp` |
| `event` | 是 | 无 | 活动 ID，>= 1 |
| `mid` | 否 | 0 | 歌曲 ID，0 或缺省为活动榜，>= 1 为歌榜 |
| `interval` | 否 | 60000 | 输出重采样间隔，毫秒，>= 1 |
| `since` | 否 | 0 | 只返回该时间之后的点，毫秒，>= 0 |

请求：

```
GET /api/eventtop/data?server=0&event=103
GET /api/eventtop/data?server=jp&event=103&mid=0&interval=60000
curl "http://127.0.0.1:8081/api/eventtop/data?server=jp&event=103&mid=0&interval=60000"
```

响应结构同 `monthlyRanking/top`。无本地数据时 302 重定向到 Bestdori。

### GET /api/tracker/data

活动榜或歌榜指定档位的分数线历史。

| 参数 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `server` | 是 | 无 | 服务器，`0` 或 `jp` |
| `event` | 是 | 无 | 活动 ID，>= 1 |
| `tier` | 是 | 无 | 档位，mid 为 0 时须在活动榜白名单，否则须在歌榜白名单 |
| `mid` | 否 | 0 | 歌曲 ID，0 或缺省为活动榜，>= 1 为歌榜 |
| `interval` | 否 | 60000 | 输出重采样间隔，毫秒，>= 1 |

请求：

```
GET /api/tracker/data?server=0&event=103&tier=1000
GET /api/tracker/data?server=jp&event=103&tier=1000&mid=0&interval=60000
curl "http://127.0.0.1:8081/api/tracker/data?server=0&event=103&tier=1000"
```

响应结构同 `monthlyRanking/border`。无本地数据时 302 重定向到 Bestdori。

### GET /api/events

活动列表。

请求：

```
GET /api/events
curl http://127.0.0.1:8081/api/events
```

无参数。

响应：对象，键为活动 ID 字符串。

```json
{
  "103": {
    "eventId": 103,
    "eventType": "versus",
    "eventName": "Roselia 対バンライブ",
    "assetBundleName": "ev103",
    "startAt": 1786671207589,
    "endAt": 1787276007589
  }
}
```

## 兼容性

| 消费方 | 说明 |
|---|---|
| Bestdori 风格调用 | `eventtop/data` 与 `tracker/data` 接受 `server=jp`、`mid=0`、`interval` 参数 |

数据只从本服务部署时刻开始累积，无历史回溯能力。需要全量历史请让消费方首次请求不带 `since`。
