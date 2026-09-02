/*
 * Penlight Dream Box / Quantumult X helper.
 *
 * Reads only the matched request URL and X-Signature header. The captured
 * values stay in Quantumult X's local persistent store and game traffic is
 * left unchanged.
 */

var STORE_KEY = "penlight-dream-box.uid-uuid.v1";

function finish() {
  $done({});
}

function readSaved() {
  var raw = $prefs.valueForKey(STORE_KEY) || "";
  if (!raw) return {};
  try {
    var value = JSON.parse(raw);
    return value && typeof value === "object" ? value : {};
  } catch (_) {
    return {};
  }
}

function writeSaved(value) {
  $prefs.setValueForKey(JSON.stringify(value), STORE_KEY);
}

function headerValue(headers, wantedName) {
  if (!headers) return "";
  var wanted = wantedName.toLowerCase();
  for (var name in headers) {
    if (!Object.prototype.hasOwnProperty.call(headers, name)) continue;
    if (name.toLowerCase() !== wanted) continue;
    var value = headers[name];
    if (Array.isArray(value)) value = value[0];
    return value == null ? "" : String(value).trim();
  }
  return "";
}

function uidFromUrl(url) {
  var match = String(url || "").match(
    /\/api\/user\/([0-9]+)(?:[\/?]|$)/i
  );
  return match ? match[1] : "";
}

function mask(value) {
  if (!value) return "未发现";
  if (value.length <= 8) return "********";
  return value.slice(0, 4) + "..." + value.slice(-4);
}

function savedJson(record) {
  return JSON.stringify(
    { uid: record.uid || "", uuid: record.uuid || "" },
    null,
    2
  );
}

function notify(title, subtitle, body, clipboardText) {
  try {
    $notify(title, subtitle, body, {
      "update-pasteboard": clipboardText || ""
    });
  } catch (_) {
    $notify(title, subtitle, body);
  }
}

function show(record) {
  var text = savedJson(record);
  if (!record.uid && !record.uuid) {
    notify(
      "Penlight Dream Box",
      "还没有捕获到 UID / UUID",
      "请打开游戏并进入个人资料或其他会访问 api.garupa.jp 的页面。",
      ""
    );
    finish();
    return;
  }

  notify(
    "Penlight Dream Box",
    "已保存 UID / UUID",
    "点击通知可复制完整 JSON\nUID: " +
      (record.uid || "未发现") +
      "\nUUID: " +
      mask(record.uuid),
    text
  );
  finish();
}

var argument =
  typeof $argument !== "undefined" && typeof $argument === "string"
    ? $argument
    : "";

if (argument === "show") {
  show(readSaved());
} else {
  var previous = readSaved();
  var uid = uidFromUrl($request && $request.url);
  var uuid = headerValue($request && $request.headers, "X-Signature");
  var current = {
    uid: uid || previous.uid || "",
    uuid: uuid || previous.uuid || ""
  };

  if (!current.uid && !current.uuid) {
    finish();
  } else {
    writeSaved(current);
    var changed =
      current.uid !== previous.uid || current.uuid !== previous.uuid;
    if (changed) {
      notify(
        "Penlight Dream Box",
        "已捕获 UID / UUID（仅本机保存）",
        "点击通知可复制完整 JSON\nUID: " +
          (current.uid || "未发现") +
          "\nUUID: " +
          mask(current.uuid),
        savedJson(current)
      );
    }
    finish();
  }
}
