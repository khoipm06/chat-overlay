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

### BUG-004: Index Out of Bounds Panic trên bảng biểu không đồng đều (Ragged Tables)
- **Thành phần**: `src/render/table.rs` (`render_markdown_table_to_pango`)
- **Mức độ**: Nghiêm trọng (Gây Crash tiến trình do Rust panic khi render table)
- **Kịch bản tái hiện**:
  1. Assistant in ra một bảng biểu Markdown có hàng dữ liệu phía sau nhiều cột hơn hàng tiêu đề đầu tiên:
     ```markdown
     | A | B |
     | --- | --- |
     | 1 |
     | 1 | 2 | 3 | 4 |
     ```
  2. Bảng biểu tính số cột theo hàng đầu tiên `let num_cols = rows[0].len()` (= 2) nhưng chỉ padding các hàng ngắn mà không cắt bớt các hàng dài hơn, khiến mảng `col_widths` có kích thước 2.
  3. Khi duyệt đến hàng có 4 cột, `col_widths[c_idx]` tại `c_idx = 2` gây `panic: index out of bounds: the len is 2 but the index is 2`.
- **Giải pháp xử lý**:
  - Tính toán `num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0)` trải rộng trên toàn bộ các hàng để hỗ trợ bảng biểu không đều (ragged table), đảm bảo `col_widths` luôn đủ phần tử.
- **Trạng thái**: ĐÃ KHẮC PHỤC. Đã thêm regression test `test_fuzz_multiline_math_and_code_blocks`.

### BUG-005: GStrInteriorNulError Panic khi stream chứa ký tự Null Byte (`\0`)
- **Thành phần**: `src/render/mod.rs` (`format_markdown_to_pango`)
- **Mức độ**: Nghiêm trọng (Gây Crash tiến trình do Rust panic khi gọi FFI GLib)
- **Kịch bản tái hiện**:
  1. Khi phản hồi từ Pi chứa ký tự nhị phân / raw control character hoặc ký tự NUL (`\0`) do in buffer thô hoặc file binary.
  2. Hàm `glib::markup_escape_text` và `pango::parse_markup` chuyển đổi chuỗi sang C string pointer `GStr`.
  3. Ký tự `\0` nằm giữa chuỗi vi phạm bất biến C-string nul-terminated, dẫn đến `panic: called Result::unwrap() on an Err value: GStrInteriorNulError`.
- **Giải pháp xử lý**:
  - Tự động làm sạch (sanitize) và loại bỏ toàn bộ ký tự `\0` ngay tại đầu vào `format_markdown_to_pango` trước khi gọi FFI GLib/Pango.
- **Trạng thái**: ĐÃ KHẮC PHỤC. Đã thêm regression test `test_fuzz_extreme_pathological_markdown_inputs`.

### BUG-006: Slicing Panic (begin <= end) trong Table Renderer khi stream ký tự pipe lẻ
- **Thành phần**: `src/render/table.rs` (`is_table_row` & `render_markdown_table_to_pango`)
- **Mức độ**: Nghiêm trọng (Gây Crash tiến trình do Rust string slicing panic trong quá trình live streaming)
- **Kịch bản tái hiện**:
  1. Khi assistant đang stream từng token và xuất hiện dòng chỉ chứa đúng 1 ký tự pipe: `|`.
  2. Điều kiện `is_table_row` cũ: `starts_with('|') && ends_with('|') && contains('|')` trả về `true` cho chuỗi `|` độ dài 1.
  3. Hàm `render_markdown_table_to_pango` thực hiện cắt chuỗi `&trimmed[1..trimmed.len() - 1]` thành `&trimmed[1..0]`, gây panic: `begin <= end (1 <= 0) when slicing |`.
- **Giải pháp xử lý**:
  - Siết chặt điều kiện: `trimmed.len() >= 2 && trimmed.starts_with('|') && trimmed.ends_with('|')`. Dòng đơn `|` không bị nhận diện nhầm là table row.
- **Trạng thái**: ĐÃ KHẮC PHỤC. Đã thêm regression test `test_fuzz_streaming_chunk_boundaries`.

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

- **Thời điểm kiểm chứng**: 2026-10-24 14:30:00 +0700
- **Commit thẩm định**: `fb31dd9` (HEAD trên branch `main`)
- **Kết quả test suite**: `cargo test --all-targets` -> **49/49 PASSED** (0 failed, 0 ignored, 0.22s).
- **Trạng thái working tree**: Clean sau khi commit biên bản.
- **Thẩm định 4 tiêu chí cốt lõi**:
  1. `Echo leak / Raw JSON filter`: **PASSED**. Dòng bắt đầu bằng `{` không lọt vào content; event nội bộ được drop an toàn.
  2. `Independent thought timer & Worked for Xs`: **PASSED**. Lưu `Instant` độc lập cho từng nhịp suy nghĩ, đóng băng thời lượng chính xác khi chuyển nhịp hoặc done; nhãn tổng kết `Worked for Xs ▾` cố định sau khi kết thúc.
  3. `Adaptive tool calls parsing`: **PASSED**. Phân tích cú pháp toàn diện cho `bash`, `read`, `edit`, `write`, `grep`, `find`, `web_search`, `fetch_content`, `source_check`, `subagent`, `todo`, `fusion_*`, `ctx_*`, `bg_*`, `mcp`, `ask_user_question`.
  4. `Antigravity agent-workspace layout`: **PASSED**. Unboxed typography hoàn chỉnh cho assistant trên toàn bộ 16 theme, elevated card cho user prompt, collapsible activity timeline chuẩn xác; bộ test suite mở rộng lên 49 tests bao phủ chuỗi ký tự code/string/comment chưa kết thúc (unterminated literals) và chuỗi hội thoại nhanh 50 turns.

**KẾT LUẬN CUỐI CÙNG**: **PASSED (CHẤP THUẬN NGHIỆM THU TOÀN DIỆN)**.

### 2. Markdown & Pango Markup Renderer (`src/render/`)
- **Kịch bản kiểm tra**:
  - Pango markup injection: Chuỗi chứa `<`, `>`, `&`, `&amp;`, thẻ HTML chưa đóng, thẻ `<script>`, tag lồng nhau.
  - Streaming safety: Code fence ` ``` ` không đóng, inline code ` ` ` chưa đóng.
  - Bảng biểu Unicode và CJK (Kanji, Kana, tiếng Việt) và emoji: Đảm bảo không vỡ Pango parser.
  - Định dạng inline & Math block phức tạp: Unclosed bold, italic, strikethrough, inline code, subscript, superscript, dollar math (`$x < y && z > 0$`, tiền tệ `$10.99`), HTML entities (`&notanentity;`), 4-5 ticks backtick lồng nhau.
  - Syntax highlighting đa ngôn ngữ (`syntax.rs`): Fuzz test Rust, Python, TypeScript, Bash, JSON, SQL, Generic XML với các token phức tạp (`Option<Vec<T>>`, docstrings, regex, escape quotes).
  - Fuzzing tổ hợp ma trận (Combinatorial stress test): Kết hợp ngẫu nhiên 144 cặp thẻ định dạng trên cùng một dòng cùng các ký tự UTF-8 / tiếng Việt / emoji.
  - Fuzzing sinh ngẫu nhiên (300 Generative Pseudo-random streams - `test_fuzz_pseudo_random_generative_markdown_stream`): Phát hiện và sửa lỗi unclosed `<kbd>` tag làm đóng sai thẻ Pango `span`, đồng thời cơ chế bảo vệ placeholder `__CODEPH__` ngăn các thẻ markdown khác đè chéo vào inline code block. Fallback an toàn bảo đảm không bao giờ vỡ giao diện.
  - Multiline math block & Ragged tables (`test_fuzz_multiline_math_and_code_blocks`): Săn tìm và triệt tiêu lỗi panic index out of bounds khi bảng biểu có số cột không đồng đều giữa các hàng, xử lý công thức toán ma trận đa dòng và ký tự LaTeX phức tạp, tích hợp khối math đa dòng `$$ ... $$` với cơ chế unclosed streaming safety.
  - Pathological Extreme Inputs (`test_fuzz_extreme_pathological_markdown_inputs`): Săn tìm và triệt tiêu lỗi panic `GStrInteriorNulError` khi stream chứa ký tự `\0`, lồng danh sách 20 bậc, ký tự điều khiển, văn bản RTL / Do Thái / Ả Rập, ký tự độ rộng bằng 0 (`\u{200B}`).
  - Massive Document Stress Test (`test_fuzz_massive_payload_stress`): Thử nghiệm tài liệu markdown khổng lồ (500 blocks, 3.500 dòng phức tạp gồm code, table, callouts, math). Toàn bộ quá trình parse & compile Pango hoàn tất dưới 220ms, Pango parse hợp lệ 100%, không tràn stack hay rò rỉ bộ nhớ.
  - LaTeX & ASCII Math Delimiters (`test_fuzz_latex_and_ascii_math_delimiters`): Kiểm tra công thức toán giới hạn `\lim`, tích phân vô hạn, tổng chuỗi `\sum_{n=1}^\infty`, tích số `\prod`, ma trận chuyển vị `\mathbf{A}^T`, tập hợp số nguyên `\mathbb{Z}` và các ký hiệu Hy Lạp.
  - Code Block Tricky Language Payloads (`test_fuzz_codeblock_nested_language_payloads`): Fuzz test các khối code có chuỗi raw string (`r###"..."###`), regex lồng nhau (`/[a-z0-9]/gi`), heredoc Bash (`cat << 'EOF'`), recursive CTE SQL, Mermaid sequence diagram và định dạng unified git diff.
  - Streaming Chunk Boundaries (`test_fuzz_streaming_chunk_boundaries`): Mô phỏng stream vi phân theo kích thước 1, 3, 7, 15, 32 ký tự cắt ngang qua header, inline code, công thức toán và bảng biểu; phát hiện và loại bỏ triệt để panic slicing tại ranh giới ký tự pipe `|`.
  - UTF-8 Multibyte Boundary Splitting (`test_fuzz_utf8_multibyte_boundary_splitting`): Kiểm thử cắt byte stream ngang qua các ký tự UTF-8 3-byte và 4-byte (tiếng Việt có dấu, Hán tự CJK, tổ hợp emoji đa thành phần); bảo đảm không làm vỡ Pango parser.
  - Raw HTML Tag Injection (`test_fuzz_html_raw_tags_and_attribute_injection`): Fuzz test các thẻ HTML nguy hiểm (`<script>`, `<iframe>`, `<img>` có handler `onerror`, `<font>`, `<input>`, tag đóng lệch `<b><i></b></i>`); toàn bộ được escape thành chuỗi XML hợp lệ.
  - URL Auto-linking & Special Schemes (`test_fuzz_url_auto_linking_and_special_schemes`): Kiểm thử liên kết với các giao thức ftp, git ssh, mailto, url encoded tiếng Việt, link có formatting lồng bên trong nhãn, cú pháp chèn ảnh `![alt](url)`.
  - Task List Hierarchy & Unicode Bullets (`test_fuzz_task_list_deep_hierarchy_and_unicode_bullets`): Kiểm thử phân cấp danh sách công việc 4 cấp độ, bullet hỗn hợp (`-` và `*`), thụt lề bằng tab `\t`, danh sách đánh số bước lớn (`999.`).
  - Code Block Language Aliases & Edge Identifiers (`test_fuzz_codeblock_language_aliases_and_edge_identifiers`): Kiểm thử 28 biến thể định danh ngôn ngữ: viết tắt (`rs`, `py`, `js`, `ts`, `sh`), ngôn ngữ hệ thống (`c`, `cpp`, `c++`, `c#`, `dockerfile`, `yaml`, `toml`), tag trống/khoảng trắng, tag viết hoa và định danh có dấu gạch ngang/nối.
  - Blockquotes Nested & Interleaved Markdown (`test_fuzz_blockquotes_nested_and_interleaved_markdown`): Kiểm thử trích dẫn lồng 3 cấp, trích dẫn xen kẽ dòng trống (`> \n >`), toàn bộ 5 biến thể GitHub Alert (`NOTE`, `TIP`, `IMPORTANT`, `WARNING`, `CAUTION`) kết hợp inline code/math/links, và các tiền tố trích dẫn thiếu dấu cách (`>Without space`, `>>Double`).
  - Header Levels & Inline Markup (`test_fuzz_header_levels_and_inline_markup_combinations`): Kiểm thử tiêu đề H1, H2, H3 kết hợp in đậm, nghiêng, code, liên kết, công thức toán $E=mc^2$, tag gạch đầu dòng trống `#`, `##`, `###` và khoảng cách bất thường.
  - HTML Details & Summary Elements (`test_fuzz_html_details_summary_variants`): Kiểm thử thẻ mở rộng `<details>` và `<summary>` lồng nhau, summary chứa bảng biểu, thẻ không đóng hoặc xuất hiện trên cùng một dòng.
  - Unicode Combining Marks & Trojan Source Bidi (`test_fuzz_codeblock_with_extreme_unicode_and_combining_marks`): Fuzz test ký tự Zalgo (`H̶e̶l̶l̶o̶`), các ký tự ghi đè hướng đọc RTL (RLO/LRO), emoji định danh biến (`🚀`, `🦀`), khoảng trắng không ngắt (NBSP), ZWJ và ký hiệu toán học Alphanumeric (`𝕏`, `ℂ`).
  - Table Nested Cells & Varied Display Widths (`test_fuzz_table_cells_with_multiline_and_nested_formatting`): Kiểm thử bảng biểu lồng chữ đậm/nghiêng/code/toán trong ô, ô trống thiếu border, cờ quốc kỳ Unicode 4-byte (`🇻🇳`, `🇯🇵`, `🇺🇸`) và bảng chỉ có đúng 1 cột.
  - Footnote & Reference Styles (`test_fuzz_footnote_and_reference_styles`): Kiểm thử trích dẫn chú thích chân trang Markdown (`[^1]`, `[^note]`), liên kết tham chiếu định danh (`[Label][ref]` & `[ref]: url`), và xử lý an toàn dấu mũ dangling.
  - Definition Lists & Abbreviations (`test_fuzz_definition_lists_and_abbreviations`): Kiểm thử định dạng danh sách định nghĩa (`Term \n : Definition`), dấu hai chấm mồ côi (`: Dangling`), và cú pháp viết tắt Markdown (`*[HTML]: ...`).
  - Nested Backticks & Fence Collision (`test_fuzz_codeblock_with_nested_backticks_and_fence_collision`): Kiểm thử khối code 4-5 dấu backtick lồng khối code con 3-4 backtick, code chứa backtick gõ lệnh shell (`whoami`), khoảng trắng sau fence đóng và ngôn ngữ có dấu chấm/phiên bản (`vue.js`, `c++20`).
  - Math Environments & Multiline Align (`test_fuzz_math_block_with_complex_environments_and_align`): Kiểm thử môi trường LaTeX `align` với dấu `&` căn lề và `\\` xuống dòng, hàm từng khúc `cases`, ma trận định thức `vmatrix` và môi trường chưa đóng.
  - Syntax Highlighter Unterminated Literals (`test_fuzz_syntax_highlighter_token_boundaries_and_unterminated_literals`): Kiểm thử bộ highlight mã nguồn khi gặp chuỗi nháy kép/đơn/template literal chưa đóng, raw string chưa đóng `r#"`, khối comment `/*` chưa đóng, số thập phân hỏng và mã hex không hợp lệ.
  - Rapid Multi-turn Dialogue Transitions (`test_fuzz_rapid_multiturn_dialogue_state_transitions`): Mô phỏng hội thoại tích lũy 50 lượt user/assistant liên tục xen kẽ khối code, công thức lũy thừa $2^n$ và đường kẻ phân cách horizontal rule `---`.
- **Kết quả**: `PASSED` (49/49 tests pass, Pango markup parser hợp lệ 100%, không panic).

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
