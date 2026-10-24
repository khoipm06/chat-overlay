# PI-OVERLAY: MASTER SPECIFICATION & CORE DESIGN ARCHITECTURE
**Document Revision:** 3.0.0 (Agent-Workspace Cloud Autonomous Tier)
**Target Platform:** Linux Desktop (Universal Wayland Layer-Shell + X11 Frameless)
**Harness Target:** Pi Coding Agent (`pi -p --mode json`)

---

## 1. TRIẾT LÝ CỐT LÕI (CORE PHILOSOPHY)

### 1.1. Lazy Senior Developer First
- Code tốt nhất là code không phải viết. Tận dụng triệt để thư viện chuẩn và native platform.
- Không phát sinh abstraction dư thừa, không boilerplates "để dành cho tương lai".
- Dọn sạch cảnh báo compiler, lint clippy `-D warnings` 100%, test tự động hóa.

### 1.2. Agent-Workspace Thu Gọn (Không Phải Messenger Chat Bubble)
- Không biến giao diện thành ứng dụng nhắn tin bong bóng to tướng (messenger bubbles).
- Không biến giao diện thành cửa sổ terminal thô kệch in raw log/raw JSON.
- Xây dựng một **Agent-Workspace thu gọn** với mật độ thông tin cao, yên tĩnh, phản ánh đúng 3 lớp:
  1. **Lớp Conversation (Hội thoại)**:
     - User request: elevated card nhẹ hoặc command row tinh tế.
     - Assistant response: unboxed text trải dài tự nhiên theo chiều rộng, font sans 14px, line-height 1.6.
  2. **Lớp Activity (Quá trình thực thi)**:
     - Gom nhóm theo từng nhịp (activity timeline).
     - Thời gian suy nghĩ (`Thought for Xs ›`): tính độc lập cho từng block suy nghĩ, KHÔNG cộng dồn.
     - Tự động thu gọn khi hoàn tất (`expanded = false`).
     - Tóm tắt tổng thời gian (`Worked for Xs ▾` / `Worked for Xm Ys ▾`) cố định sau khi Done.
     - Parse Tool Calls của Pi (`bash`, `read`, `edit`, `write`, `grep`, `find`) thành tên hành động trực quan (`Ran command: cargo test ›`, `Read file: src/main.rs ›`, `Edited file: src/overlay.rs ›`). Raw arguments chỉ xem khi bấm mở rộng.
  3. **Lớp Deliverables / Artifacts**:
     - Các đầu ra có cấu trúc (Implementation plan, Task list, Unified Diff, Test results, File exports).
     - Hiển thị dạng card tóm tắt kèm nút mở chi tiết.

### 1.3. Tính Phổ Quát (Universal Linux Desktop)
- Hoạt động ổn định trên TẤT CẢ các DE/WM: KDE Plasma, GNOME, Hyprland, Sway, River, XFCE, i3, COSMIC.
- KHÔNG gọi hardcoded các công cụ độc quyền của một desktop environment nào nếu không phát hiện `XDG_CURRENT_DESKTOP`.
- Độ sâu tầng hiển thị: `Layer::Top` kết hợp `exclusive_zone = -1` để không đè lên screenshot tool / lockscreen và có thể trượt mượt mà ra sau taskbar khi kéo xuống sát đáy.
- Cô lập CSS triệt để: không bị nhiễm màu/halo/bóng đổ từ theme hệ thống của distro.
- Resize 8 hướng (4 cạnh N, S, E, W + 4 góc chéo NW, NE, SW, SE) mượt mà 60/120Hz, không lag, không ghosting.

---

## 2. QUY CHUẨN MÀU SẮC & GIAO DIỆN (ANTIGRAVITY NEUTRAL PALETTE)

```css
:root {
  --surface-app: #18191b;
  --surface-raised: #202226;
  --surface-hover: #292c31;
  --surface-code: #141517;
  --border-subtle: rgba(255, 255, 255, 0.08);
  --border-focus: #3b82f6;

  --text-primary: #e8e9ed;
  --text-secondary: #a5aab4;
  --text-muted: #777f8c;

  --accent: #8ab4f8;
  --success: #81c995;
  --warning: #f2c66d;
  --danger: #f28b82;

  --radius-card: 8px;
  --radius-composer: 10px;
}
```

---

## 3. CƠ CHẾ KIỂM CHỨNG ĐỘC LẬP (INDEPENDENT VERIFICATION)
- **Điều kiện hoàn thành không phải là `cargo test pass`** mà phải được **Verifier Agent độc lập** kiểm tra thực tế:
  1. Kiểm tra mã nguồn diff: không có echo leak, không có regression.
  2. Kiểm tra parser tool: không in raw JSON `{"command": ...}` ra ngoài nhãn hiển thị.
  3. Kiểm tra logic timer: nhịp suy nghĩ nào đo thời gian nhịp đó.
  4. Kiểm tra CSS: các dropdown và popover hoàn toàn dark mode, không viền xanh halo, không nền trắng.
  5. Đạt chuẩn: Verifier ký duyệt `PASSED`. Nếu chưa đạt: gửi feedback chi tiết cho Worker sửa lại.
