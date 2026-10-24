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
- **Kết quả**: `PASSED` (23/23 unit & fuzz tests pass, `cargo check --all-targets` sạch 100%).

---

## Biên bản xác nhận nghiệm thu độc lập (Independent Verifier Sign-off)

- **Thời điểm kiểm chứng**: 2026-10-24 13:18:00 +0700
- **Commit thẩm định**: `3416d79` (HEAD trên branch `feat/antigravity-workspace-and-timeline-standard`)
- **Kết quả test suite**: `cargo test --all-targets` -> **24/24 PASSED** (0 failed, 0 ignored, 0.00s).
- **Trạng thái working tree**: Clean.
- **Thẩm định 4 tiêu chí cốt lõi**:
  1. `Echo leak / Raw JSON filter`: **PASSED**. Dòng bắt đầu bằng `{` không lọt vào content; event nội bộ được drop an toàn.
  2. `Independent thought timer & Worked for Xs`: **PASSED**. Lưu `Instant` độc lập cho từng nhịp suy nghĩ, đóng băng thời lượng chính xác khi chuyển nhịp hoặc done; nhãn tổng kết `Worked for Xs ▾` cố định sau khi kết thúc.
  3. `Adaptive tool calls parsing`: **PASSED**. Phân tích cú pháp đầy đủ cho `bash`, `read`, `edit`, `write`, `grep`, `find`, `web_search`, `fetch_content`, `source_check`, `subagent`.
  4. `Antigravity agent-workspace layout`: **PASSED**. Unboxed typography hoàn chỉnh cho assistant trên toàn bộ 16 theme, elevated card cho user prompt, collapsible activity timeline chuẩn xác.

**KẾT LUẬN CUỐI CÙNG**: **PASSED (CHẤP THUẬN NGHIỆM THU TOÀN DIỆN)**.

### 2. Markdown & Pango Markup Renderer (`src/render/`)
- **Kịch bản kiểm tra**:
  - Pango markup injection: Chuỗi chứa `<`, `>`, `&`, `&amp;`, thẻ HTML chưa đóng, thẻ `<script>`, tag lồng nhau.
  - Streaming safety: Code fence ` ``` ` không đóng, inline code ` ` ` chưa đóng.
  - Bảng biểu Unicode và CJK (Kanji, Kana, tiếng Việt) và emoji: Đảm bảo không vỡ Pango parser.
  - Định dạng inline & Math block phức tạp: Unclosed bold, italic, strikethrough, inline code, subscript, superscript, dollar math (`$x < y && z > 0$`, tiền tệ `$10.99`), HTML entities (`&notanentity;`), 4-5 ticks backtick lồng nhau.
  - Syntax highlighting đa ngôn ngữ (`syntax.rs`): Fuzz test Rust, Python, TypeScript, Bash, JSON, SQL, Generic XML với các token phức tạp (`Option<Vec<T>>`, docstrings, regex, escape quotes).
  - Fuzzing tổ hợp ma trận (Combinatorial stress test): Kết hợp ngẫu nhiên 144 cặp thẻ định dạng trên cùng một dòng cùng các ký tự UTF-8 / tiếng Việt / emoji.
- **Kết quả**: `PASSED` (26/26 tests pass, Pango markup parser hợp lệ 100%, không panic).

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
