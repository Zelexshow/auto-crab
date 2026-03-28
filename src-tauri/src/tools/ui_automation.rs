#[cfg(target_os = "windows")]
use anyhow::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiNode {
    pub role: String,
    pub name: String,
    pub rect: Option<[i32; 4]>,
    pub states: Vec<String>,
    pub children: Vec<UiNode>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiTreeSnapshot {
    pub window_title: String,
    pub window_rect: [i32; 4],
    pub tree: Vec<UiNode>,
}

impl UiTreeSnapshot {
    pub fn serialize_text(&self) -> String {
        let mut out = format!("[Window] {} ({},{} {}x{})\n",
            self.window_title, self.window_rect[0], self.window_rect[1],
            self.window_rect[2], self.window_rect[3]);
        for node in &self.tree {
            serialize_node(&mut out, node, 1);
        }
        out
    }

    pub fn has_useful_elements(&self) -> bool {
        let total = count_nodes(&self.tree);
        let with_name = count_with_name(&self.tree);
        total >= 3 && (with_name as f64 / total.max(1) as f64) > 0.2
    }
}

fn count_nodes(nodes: &[UiNode]) -> usize {
    nodes.iter().map(|n| 1 + count_nodes(&n.children)).sum()
}

fn count_with_name(nodes: &[UiNode]) -> usize {
    nodes.iter().map(|n| {
        let this = if !n.name.is_empty() { 1 } else { 0 };
        this + count_with_name(&n.children)
    }).sum()
}

fn serialize_node(out: &mut String, node: &UiNode, depth: usize) {
    let indent = "│  ".repeat(depth.saturating_sub(1));
    let prefix = if depth > 0 { "├─ " } else { "" };

    let rect_str = node.rect.map(|r| format!(" ({},{} {}x{})", r[0], r[1], r[2], r[3]))
        .unwrap_or_default();

    let states_str = if node.states.is_empty() { String::new() }
        else { format!(" {{{}}}", node.states.join(", ")) };

    let name_display = if node.name.len() > 50 {
        format!("{}...", &node.name[..47])
    } else {
        node.name.clone()
    };

    out.push_str(&format!("{}{}{} [{}]{}{}\n",
        indent, prefix, name_display, node.role, rect_str, states_str));

    for child in &node.children {
        serialize_node(out, child, depth + 1);
    }
}

#[cfg(target_os = "windows")]
pub fn get_foreground_ui_tree(max_depth: u32) -> Result<UiTreeSnapshot> {
    use windows::Win32::UI::Accessibility::*;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::Foundation::*;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let uia: IUIAutomation = CoCreateInstance(
            &CUIAutomation,
            None,
            CLSCTX_INPROC_SERVER,
        )?;

        let hwnd = GetForegroundWindow();
        let element = uia.ElementFromHandle(hwnd)?;

        let title = element.CurrentName()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "Unknown".into());

        let rect = get_rect(&element);

        let tree = walk_element(&uia, &element, 0, max_depth)?;

        Ok(UiTreeSnapshot {
            window_title: title,
            window_rect: rect,
            tree,
        })
    }
}

#[cfg(target_os = "windows")]
pub fn focus_window_by_title(title: &str) -> Result<String> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::*;

    let title_lower = title.to_lowercase();
    let aliases: Vec<String> = match title_lower.as_str() {
        "记事本" => vec!["记事本".into(), "notepad".into(), "无标题".into()],
        "notepad" => vec!["notepad".into(), "记事本".into(), "无标题".into()],
        "微信" => vec!["微信".into(), "wechat".into(), "weixin".into()],
        "飞书" => vec!["飞书".into(), "feishu".into(), "lark".into()],
        "浏览器" | "chrome" => vec!["chrome".into(), "edge".into(), "firefox".into(), "浏览器".into()],
        _ => vec![title_lower.clone()],
    };

    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        unsafe {
            let mut found: Option<HWND> = None;
            for alias in &aliases {
                if found.is_some() { break; }
                let _ = EnumWindows(Some(enum_callback), LPARAM(&mut (alias.clone(), &mut found) as *mut _ as isize));
            }

            if let Some(hwnd) = found {
                let _ = SetForegroundWindow(hwnd);
                let _ = ShowWindow(hwnd, SW_RESTORE);
                std::thread::sleep(std::time::Duration::from_millis(500));

                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut buf);
                let win_title = String::from_utf16_lossy(&buf[..len as usize]);
                return Ok(format!("已聚焦窗口: {}", win_title));
            }
        }
    }

    anyhow::bail!("未找到包含 '{}' 的窗口（已重试3次）", title)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_callback(hwnd: windows::Win32::Foundation::HWND, lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::BOOL {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Foundation::BOOL;

    let data = &mut *(lparam.0 as *mut (String, &mut Option<windows::Win32::Foundation::HWND>));
    let (ref search, ref mut result) = data;

    if result.is_some() {
        return BOOL(0);
    }

    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }

    let mut buf = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len > 0 {
        let title = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
        if title.contains(search.as_str()) {
            **result = Some(hwnd);
            return BOOL(0);
        }
    }
    BOOL(1)
}

#[cfg(target_os = "windows")]
unsafe fn get_rect(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> [i32; 4] {
    element.CurrentBoundingRectangle()
        .map(|r| [r.left, r.top, r.right - r.left, r.bottom - r.top])
        .unwrap_or([0, 0, 0, 0])
}

#[cfg(target_os = "windows")]
unsafe fn walk_element(
    uia: &windows::Win32::UI::Accessibility::IUIAutomation,
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
    depth: u32,
    max_depth: u32,
) -> Result<Vec<UiNode>> {
    use windows::Win32::UI::Accessibility::*;

    if depth >= max_depth {
        return Ok(vec![]);
    }

    let mut nodes = Vec::new();
    let condition = uia.CreateTrueCondition()?;

    if let Ok(children) = element.FindAll(TreeScope_Children, &condition) {
        let count = children.Length().unwrap_or(0);
        let limit = count.min(50);

        for i in 0..limit {
            if let Ok(child) = children.GetElement(i) {
                let name = child.CurrentName()
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                let control_type = child.CurrentControlType().unwrap_or_default();
                let role = control_type_name(control_type);

                let rect = get_rect(&child);
                let rect_opt = if rect[2] > 0 && rect[3] > 0 { Some(rect) } else { None };

                let mut states = Vec::new();
                if child.CurrentIsEnabled().unwrap_or_default().as_bool() {
                    if matches!(role, "Button" | "MenuItem" | "ListItem" | "TabItem" | "Hyperlink") {
                        states.push("clickable".into());
                    }
                    if role == "Edit" || role == "Document" {
                        states.push("editable".into());
                    }
                }

                let sub = walk_element(uia, &child, depth + 1, max_depth)?;

                if !name.is_empty() || !sub.is_empty() || !states.is_empty() {
                    nodes.push(UiNode {
                        role: role.to_string(),
                        name,
                        rect: rect_opt,
                        states,
                        children: sub,
                    });
                }
            }
        }
    }

    Ok(nodes)
}

#[cfg(target_os = "windows")]
fn control_type_name(ct: windows::Win32::UI::Accessibility::UIA_CONTROLTYPE_ID) -> &'static str {
    use windows::Win32::UI::Accessibility::*;
    match ct {
        UIA_ButtonControlTypeId => "Button",
        UIA_CalendarControlTypeId => "Calendar",
        UIA_CheckBoxControlTypeId => "CheckBox",
        UIA_ComboBoxControlTypeId => "ComboBox",
        UIA_EditControlTypeId => "Edit",
        UIA_HyperlinkControlTypeId => "Hyperlink",
        UIA_ImageControlTypeId => "Image",
        UIA_ListItemControlTypeId => "ListItem",
        UIA_ListControlTypeId => "List",
        UIA_MenuControlTypeId => "Menu",
        UIA_MenuBarControlTypeId => "MenuBar",
        UIA_MenuItemControlTypeId => "MenuItem",
        UIA_ProgressBarControlTypeId => "ProgressBar",
        UIA_RadioButtonControlTypeId => "RadioButton",
        UIA_ScrollBarControlTypeId => "ScrollBar",
        UIA_SliderControlTypeId => "Slider",
        UIA_TabControlTypeId => "Tab",
        UIA_TabItemControlTypeId => "TabItem",
        UIA_TextControlTypeId => "Text",
        UIA_ToolBarControlTypeId => "ToolBar",
        UIA_TreeControlTypeId => "Tree",
        UIA_TreeItemControlTypeId => "TreeItem",
        UIA_WindowControlTypeId => "Window",
        UIA_PaneControlTypeId => "Pane",
        UIA_GroupControlTypeId => "Group",
        UIA_DocumentControlTypeId => "Document",
        UIA_StatusBarControlTypeId => "StatusBar",
        UIA_TitleBarControlTypeId => "TitleBar",
        UIA_HeaderControlTypeId => "Header",
        UIA_DataGridControlTypeId => "DataGrid",
        UIA_DataItemControlTypeId => "DataItem",
        UIA_CustomControlTypeId => "Custom",
        UIA_SplitButtonControlTypeId => "SplitButton",
        UIA_TableControlTypeId => "Table",
        UIA_ToolTipControlTypeId => "ToolTip",
        UIA_ThumbControlTypeId => "Thumb",
        UIA_SeparatorControlTypeId => "Separator",
        _ => "Unknown",
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_foreground_ui_tree(_max_depth: u32) -> anyhow::Result<UiTreeSnapshot> {
    anyhow::bail!("UI Automation is only supported on Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn focus_window_by_title(_title: &str) -> anyhow::Result<String> {
    anyhow::bail!("focus_window is only supported on Windows")
}
