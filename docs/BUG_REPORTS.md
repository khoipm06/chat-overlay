# QA & Bug Hunter Report

Hệ thống ghi nhận và theo dõi các lỗi biên, regression, rò rỉ bộ nhớ, zombie process và tương thích GTK4 CSS.

---

## Báo cáo kiểm thử đợt 1 (Đã kiểm tra & xử lý)

### BUG-001: Rò rỉ raw JSON do Fallback Dispatcher trong NDJSON streaming parser
- **Thành phần**: `src/backend.rs` (`parse_and_dispatch_line`)
- **Mức độ**: Nghiêm trọng (Gây vỡ UX, vi phạm Section 1.2 MASTER_SPEC)
- **Kịch bản tái hiện**:
  1. Khi Pi gửi các event nội bộ không được xử lý cụ thể trong match (như `turn_start`, `turn_end`, `agent_start`) hoặc JSON lỗi cú pháp.
  2. Đoạn fallback cũ tự động coi mọi dòng không match là plain text content và gửi `StreamEvent::Content(raw_line)` hiển thị ra giao diện người dùng.
- **Hiện tượng**:
  - Raw JSON string `{"type":"turn_start", ...}` bị in thẳng vào bong bóng hội thoại.
- **Giải pháp xử lý**:
  - Chỉ gửi `StreamEvent::Content` khi dòng văn bản không bắt đầu bằng ký tự `{`.
  - Toàn bộ event JSON nội bộ không cần hiển thị được drop an toàn trong match `_ => {}`.
- **Trạng thái**: ĐÃ KHẮC PHỤC. Thêm regression test `test_parse_jsonl_fuzz_and_edge_cases`.

---

## Báo cáo fuzzing & stress test hiện tại

### 1. NDJSON Streaming Parser (`src/backend.rs`)
- **Kịch bản kiểm tra**:
  - Tool call lồng nhau phức tạp (nested JSON object, diff, source code chứa newline và ký tự escape).
  - Unicode ký tự lạ và tiếng Việt có dấu (`Tiếng Việt 🦀`, path có dấu `/đường_dẫn/đến/tệp_tin.txt`).
  - Event `tool_execution_start` thiếu field `args`.
  - JSON lỗi cú pháp / dòng JSON bị cắt cụt.
- **Kết quả**: `PASSED` (21/21 unit & fuzz tests pass, Clippy sạch cảnh báo).

### 2. Markdown & Pango Markup Renderer (`src/render/`)
- **Kịch bản kiểm tra**:
  - Pango markup injection: Chuỗi chứa `<`, `>`, `&`, `&amp;`, thẻ HTML chưa đóng, thẻ `<script>`, tag lồng nhau.
  - Streaming safety: Code fence ` ``` ` không đóng, inline code ` ` ` chưa đóng.
  - Bảng biểu Unicode và CJK (Kanji, Kana, tiếng Việt) và emoji: Đảm bảo không vỡ Pango parser.
- **Kết quả**: `PASSED` (Pango parse hợp lệ 100%, không bị panic, fallback graceful sang plain text qua `apply_assistant_markup` nếu có lỗi parse).

### 3. Quản lý tiến trình & Zombie Process
- **Cơ chế**:
  - Hàm `PiRunner::abort()` khi spawn hoặc huỷ tác vụ chủ động gọi `tokio::spawn` thực hiện `child.kill().await` và `child.wait().await`.
  - Đã kiểm tra qua `ps -ef`: Không có tiến trình zombie / orphaned process `pi` hoặc `pi-overlay`.
- **Kết quả**: `PASSED`.

### 4. GTK4 CSS Runtime Parser
- **Tình trạng**:
  - File `resources/style.css` được nhúng trực tiếp qua `include_bytes!`.
  - Không có cảnh báo CSS parsing error tại runtime. Toàn bộ 115 class selector và 16 theme hoạt động chính xác.
- **Kết quả**: `PASSED`.
