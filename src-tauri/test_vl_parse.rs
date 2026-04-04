fn main() {
    let input = r#"{
  "screen_description": "微信窗口已打开",
  "task_status": "need_action",
  "actions": [
    {
      "type": "click",
      "x": 412, 798,
      "reason": "点击输入框"
    },
    {
      "type": "type",
      "text": "泪目啊",
      "reason": "输入文本"
    },
    {
      "type": "click",
      "x": 780, 800,
      "reason": "点击发送按钮"
    }
  ]
}"#;
    let re = regex::Regex::new(r#""x"\s*:\s*(\d+)\s*,\s*(\d+)"#).unwrap();
    let mut fixed = re.replace_all(input, r#""x": $1, "y": $2"#).to_string();
    fixed = fixed.replace(",]", "]").replace(",}", "}");
    let parsed: serde_json::Value = serde_json::from_str(&fixed).unwrap();
    let actions = parsed["actions"].as_array().unwrap();
    for a in actions {
        println!("type={}, x={}, y={}, text={}", 
            a["type"].as_str().unwrap_or(""),
            a["x"].as_i64().unwrap_or(-1),
            a["y"].as_i64().unwrap_or(-1),
            a["text"].as_str().unwrap_or(""));
    }
    println!("OK! {} actions parsed", actions.len());
}
