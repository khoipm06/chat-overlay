//! Dynamic UI Scaling and Typography Management
//!
//! Generates and injects GTK4 CSS overrides to proportionally scale typography,
//! buttons, padding, icons, code blocks, tables, and tap targets across the application.

fn scaled(val: f64, factor: f64) -> f64 {
    (val * factor * 10.0).round() / 10.0
}

fn scaled_min(val: f64, factor: f64, min_val: f64) -> f64 {
    let s = (val * factor * 10.0).round() / 10.0;
    if s < min_val {
        min_val
    } else {
        s
    }
}

/// Generates GTK4 CSS rules to scale UI components proportionally.
pub fn generate_scale_css(scale_percent: u32, base_font_size: u32) -> String {
    let scale_percent = scale_percent.clamp(80, 250);
    let base_font_size = base_font_size.clamp(9, 32);
    let factor = (scale_percent as f64) / 100.0;
    let base_fs = ((base_font_size as f64) * factor * 10.0).round() / 10.0;

    let s = |v: f64| scaled(v, factor);
    let sm = |v: f64, min_v: f64| scaled_min(v, factor, min_v);

    format!(
        r#"/* ── Dynamic UI Scale & Typography Provider ({scale_percent}%, base {base_fs}px) ── */
* {{
    font-size: {base_fs}px;
}}

#drawer-container,
.drawer-container,
.window-container {{
    font-size: {base_fs}px;
}}

.panel-header {{
    padding: {header_pad_y}px {header_pad_x}px;
    min-height: {header_h}px;
}}

.panel-title {{
    font-size: {panel_title_fs}px;
}}

.header-icon-btn {{
    min-width: {header_btn_size}px;
    min-height: {header_btn_size}px;
    padding: {header_btn_pad_y}px {header_btn_pad_x}px;
    font-size: {header_btn_fs}px;
    border-radius: {header_btn_rad}px;
}}

.session-subheader {{
    padding: {subhead_pad_y}px {subhead_pad_x}px;
    min-height: {subhead_h}px;
}}

.session-back-btn {{
    min-width: {back_btn_size}px;
    min-height: {back_btn_size}px;
    padding: {back_btn_pad}px;
    font-size: {back_btn_fs}px;
    border-radius: {back_btn_rad}px;
}}

.session-title-label {{
    font-size: {session_title_fs}px;
}}

.chat-scroll {{
    padding: {chat_scroll_pad_y}px {chat_scroll_pad_x}px;
}}

.welcome-hero {{
    padding: {welcome_pad_y}px {welcome_pad_x}px;
}}

.welcome-glyph {{
    font-size: {welcome_glyph_fs}px;
    margin-bottom: {welcome_glyph_mb}px;
}}

.welcome-title {{
    font-size: {welcome_title_fs}px;
    margin-bottom: {welcome_title_mb}px;
}}

.welcome-subtitle {{
    font-size: {welcome_sub_fs}px;
    margin-bottom: {welcome_sub_mb}px;
}}

.welcome-pill-btn {{
    font-size: {welcome_pill_fs}px;
    padding: {welcome_pill_pad_y}px {welcome_pill_pad_x}px;
    border-radius: {welcome_pill_rad}px;
    min-height: {welcome_pill_h}px;
}}

.card-user {{
    padding: {card_user_pad_y}px {card_user_pad_x}px;
    border-radius: {card_user_rad}px;
    margin-bottom: {card_user_mb}px;
}}

.card-user-badge {{
    font-size: {card_user_badge_fs}px;
}}

.card-user-idx {{
    font-size: {card_user_idx_fs}px;
}}

.card-user-text {{
    font-size: {base_fs}px;
    line-height: 1.4;
}}

.card-assistant {{
    padding: {card_asst_pad_y}px 0;
    margin-bottom: {card_asst_mb}px;
}}

.card-assistant-avatar {{
    font-size: {asst_avatar_fs}px;
    margin-right: {asst_avatar_mr}px;
}}

.card-assistant-badge {{
    font-size: {asst_badge_fs}px;
}}

.card-metrics-pill {{
    font-size: {metrics_pill_fs}px;
    padding: {metrics_pill_pad_y}px {metrics_pill_pad_x}px;
    border-radius: {metrics_pill_rad}px;
    margin-left: {metrics_pill_ml}px;
}}

.bubble-assistant-text {{
    font-size: {base_fs}px;
    line-height: 1.4;
}}

.card-assistant tt {{
    font-size: {asst_tt_fs}px;
    padding: {asst_tt_pad_y}px {asst_tt_pad_x}px;
    border-radius: {asst_tt_rad}px;
}}

.progress-steps-disclosure {{
    margin: {disc_m_top}px 0 {disc_m_bot}px 0;
    border-radius: {disc_rad}px;
    padding: {disc_pad_y}px {disc_pad_x}px;
}}

.progress-steps-disclosure > title {{
    font-size: {disc_title_fs}px;
    padding: {disc_title_pad_y}px {disc_title_pad_x}px;
}}

.steps-list-box {{
    margin-top: {steps_mt}px;
    padding: {steps_py}px 0;
}}

.step-item-card {{
    padding: {step_card_pad_y}px {step_card_pad_x}px;
    margin: {step_card_my}px 0;
    border-radius: {step_card_rad}px;
}}

.step-item-icon {{
    font-size: {step_icon_fs}px;
    margin-right: {step_icon_mr}px;
}}

.step-item-label {{
    font-size: {step_label_fs}px;
}}

.step-item-file-chip {{
    font-size: {step_chip_fs}px;
    padding: {step_chip_pad_y}px {step_chip_pad_x}px;
    border-radius: {step_chip_rad}px;
    margin-left: {step_chip_ml}px;
}}

.step-item-status {{
    font-size: {step_status_fs}px;
    padding: {step_status_pad_y}px {step_status_pad_x}px;
    border-radius: {step_status_rad}px;
}}

.tool-call-card {{
    border-radius: {tool_card_rad}px;
    margin: {tool_card_my}px 0;
    padding: {tool_card_pad_y}px {tool_card_pad_x}px;
}}

.tool-name-label {{
    font-size: {tool_name_fs}px;
}}

.tool-badge-running,
.tool-badge-done,
.tool-badge-error {{
    font-size: {tool_badge_fs}px;
    border-radius: {tool_badge_rad}px;
    padding: {tool_badge_pad_y}px {tool_badge_pad_x}px;
}}

.tool-args-box {{
    margin-top: {tool_args_mt}px;
    font-size: {tool_args_fs}px;
    border-radius: {tool_args_rad}px;
    padding: {tool_args_pad_y}px {tool_args_pad_x}px;
}}

.tool-output-expander > title {{
    font-size: {tool_out_title_fs}px;
}}

.tool-output-box {{
    margin-top: {tool_out_mt}px;
    font-size: {tool_out_fs}px;
    border-radius: {tool_out_rad}px;
    padding: {tool_out_pad_y}px {tool_out_pad_x}px;
}}

.tool-output-label {{
    font-size: {tool_out_fs}px;
}}

.card-feedback-bar {{
    margin-top: {feedback_mt}px;
    padding-top: {feedback_pt}px;
}}

.feedback-btn {{
    min-width: {feedback_btn_size}px;
    min-height: {feedback_btn_size}px;
    padding: {feedback_btn_pad_y}px {feedback_btn_pad_x}px;
    border-radius: {feedback_btn_rad}px;
    font-size: {feedback_btn_fs}px;
}}

.btn-scroll-bottom {{
    min-width: {scroll_btn_size}px;
    min-height: {scroll_btn_size}px;
    border-radius: {scroll_btn_rad}px;
    font-size: {scroll_btn_fs}px;
}}

.composer-container {{
    border-radius: {comp_container_rad}px;
    padding: {comp_container_pad_y}px {comp_container_pad_x}px;
    margin: {comp_container_my_top}px {comp_container_mx}px {comp_container_my_bot}px {comp_container_mx}px;
}}

.composer-chips-row {{
    margin-bottom: {comp_chips_mb}px;
}}

.composer-chip {{
    border-radius: {comp_chip_rad}px;
    padding: {comp_chip_pad_y}px {comp_chip_pad_x}px;
    margin-right: {comp_chip_mr}px;
    margin-bottom: {comp_chip_mb}px;
}}

.composer-chip-icon {{
    font-size: {comp_chip_icon_fs}px;
    margin-right: {comp_chip_icon_mr}px;
}}

.composer-chip-text {{
    font-size: {comp_chip_text_fs}px;
}}

.composer-chip-del {{
    min-width: {comp_chip_del_size}px;
    min-height: {comp_chip_del_size}px;
    border-radius: {comp_chip_del_rad}px;
    margin-left: {comp_chip_del_ml}px;
    font-size: {comp_chip_del_fs}px;
}}

.composer-text-view {{
    font-size: {base_fs}px;
    line-height: 1.4;
    padding: {comp_tv_pad_y}px 0;
    min-height: {comp_tv_min_h}px;
}}

.composer-placeholder {{
    font-size: {base_fs}px;
}}

.composer-toolbar {{
    margin-top: {comp_toolbar_mt}px;
    padding-top: {comp_toolbar_pt}px;
}}

.composer-pill-btn {{
    border-radius: {comp_pill_rad}px;
    padding: {comp_pill_pad_y}px {comp_pill_pad_x}px;
    font-size: {comp_pill_fs}px;
    margin-right: {comp_pill_mr}px;
    min-height: {comp_pill_h}px;
}}

.composer-icon-btn {{
    min-width: {comp_icon_btn_size}px;
    min-height: {comp_icon_btn_size}px;
    padding: {comp_icon_btn_pad}px;
    border-radius: {comp_icon_btn_rad}px;
    font-size: {comp_icon_btn_fs}px;
    margin-right: {comp_icon_btn_mr}px;
}}

.composer-send-btn {{
    min-width: {comp_send_btn_size}px;
    min-height: {comp_send_btn_size}px;
    border-radius: {comp_send_btn_rad}px;
    font-size: {comp_send_btn_fs}px;
}}

.composer-send-btn.stop {{
    font-size: {comp_send_stop_fs}px;
}}

.status-footer {{
    padding: {status_pad_top}px {status_pad_side}px {status_pad_bot}px {status_pad_side}px;
}}

.status-footer-label {{
    font-size: {status_label_fs}px;
}}

.status-footer-btn {{
    border-radius: {status_btn_rad}px;
    padding: {status_btn_pad_y}px {status_btn_pad_x}px;
    font-size: {status_btn_fs}px;
    min-height: {status_btn_h}px;
}}

.status-footer-dot-sep {{
    font-size: {status_sep_fs}px;
    margin: 0 {status_sep_mx}px;
}}

.status-footer-icon {{
    font-size: {status_icon_fs}px;
    margin-right: {status_icon_mr}px;
}}

.status-footer-dot {{
    min-width: {status_dot_size}px;
    min-height: {status_dot_size}px;
    border-radius: {status_dot_rad}px;
}}

.clean-popover > contents {{
    border-radius: {pop_contents_rad}px;
    padding: {pop_contents_pad}px;
}}

.clean-popover-title {{
    font-size: {pop_title_fs}px;
    padding: {pop_title_pad_top}px {pop_title_pad_x}px {pop_title_pad_bot}px {pop_title_pad_x}px;
    margin-bottom: {pop_title_mb}px;
}}

.clean-popover-item {{
    padding: {pop_item_pad_y}px {pop_item_pad_x}px;
    border-radius: {pop_item_rad}px;
    font-size: {pop_item_fs}px;
    margin-bottom: {pop_item_mb}px;
    min-height: {pop_item_min_h}px;
}}

.clean-popover-item-desc {{
    font-size: {pop_desc_fs}px;
}}

.clean-popover-check {{
    padding: {pop_check_pad_y}px {pop_check_pad_x}px;
    font-size: {pop_check_fs}px;
    min-height: {pop_check_min_h}px;
}}

.clean-popover-check check {{
    min-width: {pop_check_box_size}px;
    min-height: {pop_check_box_size}px;
    border-radius: {pop_check_box_rad}px;
}}

.prompt-row-box {{
    padding: {prompt_row_pad_y}px {prompt_row_pad_x}px;
    border-radius: {prompt_row_rad}px;
}}

.prompt-idx-pill {{
    font-size: {prompt_pill_fs}px;
    border-radius: {prompt_pill_rad}px;
    padding: {prompt_pill_pad_y}px {prompt_pill_pad_x}px;
    margin-right: {prompt_pill_mr}px;
}}

.prompt-snippet-text {{
    font-size: {prompt_snip_fs}px;
}}

.prompt-delete-btn {{
    min-width: {prompt_del_size}px;
    min-height: {prompt_del_size}px;
    padding: {prompt_del_pad_y}px {prompt_del_pad_x}px;
    border-radius: {prompt_del_rad}px;
    font-size: {prompt_del_fs}px;
    margin-left: {prompt_del_ml}px;
}}

.confirm-popover > contents {{
    border-radius: {confirm_rad}px;
    padding: {confirm_pad_y}px {confirm_pad_x}px;
}}

.confirm-dialog-title {{
    font-size: {confirm_title_fs}px;
    margin-bottom: {confirm_title_mb}px;
}}

.confirm-dialog-desc {{
    font-size: {confirm_desc_fs}px;
    margin-bottom: {confirm_desc_mb}px;
    line-height: 1.4;
}}

.btn-dialog-destructive {{
    font-size: {confirm_btn_fs}px;
    padding: {confirm_btn_pad_y}px {confirm_btn_pad_x}px;
    border-radius: {confirm_btn_rad}px;
    min-height: {confirm_btn_h}px;
}}

.btn-dialog-cancel {{
    font-size: {confirm_btn_fs}px;
    padding: {confirm_btn_pad_y}px {confirm_btn_pad_x}px;
    border-radius: {confirm_btn_rad}px;
    min-height: {confirm_btn_h}px;
}}

.code-block-container {{
    border-radius: {code_box_rad}px;
    margin: {code_box_my}px 0;
}}

.code-block-header {{
    padding: {code_hdr_pad_y}px {code_hdr_pad_x}px;
    border-top-left-radius: {code_hdr_rad}px;
    border-top-right-radius: {code_hdr_rad}px;
}}

.code-block-lang {{
    font-size: {code_lang_fs}px;
}}

.code-block-copy-btn {{
    font-size: {code_copy_fs}px;
    padding: {code_copy_pad_y}px {code_copy_pad_x}px;
    border-radius: {code_copy_rad}px;
    min-height: {code_copy_min_h}px;
}}

.code-block-content {{
    padding: {code_body_pt}px {code_body_px}px {code_body_pb}px {code_body_px}px;
    font-size: {code_body_fs}px;
    line-height: 1.4;
}}

.markdown-table-card {{
    border-radius: {tbl_card_rad}px;
    margin: {tbl_card_my}px 0;
}}

.table-cell-header {{
    padding: {tbl_hdr_pad_y}px {tbl_hdr_pad_x}px;
}}

.table-header-text {{
    font-size: {tbl_hdr_fs}px;
}}

.table-cell {{
    padding: {tbl_cell_pad_y}px {tbl_cell_pad_x}px;
}}

.table-cell-text {{
    font-size: {tbl_cell_fs}px;
    line-height: 1.4;
}}

.table-cell-text tt {{
    font-size: {tbl_tt_fs}px;
    border-radius: {tbl_tt_rad}px;
    padding: {tbl_tt_pad_y}px {tbl_tt_pad_x}px;
}}

.chat-head-btn {{
    min-width: {head_btn_size}px;
    min-height: {head_btn_size}px;
    border-radius: {head_btn_rad}px;
}}

.chat-head-icon {{
    font-size: {head_icon_fs}px;
}}

.chat-head-dot {{
    min-width: {head_dot_size}px;
    min-height: {head_dot_size}px;
    border-radius: {head_dot_rad}px;
}}
"#,
        header_pad_y = s(3.0),
        header_pad_x = s(8.0),
        header_h = sm(28.0, 28.0),
        panel_title_fs = sm(12.0, 12.0),
        header_btn_size = sm(24.0, 24.0),
        header_btn_pad_y = s(2.0),
        header_btn_pad_x = s(5.0),
        header_btn_fs = sm(12.5, 12.0),
        header_btn_rad = s(4.0),
        subhead_pad_y = s(2.5),
        subhead_pad_x = s(8.0),
        subhead_h = sm(26.0, 26.0),
        back_btn_size = sm(24.0, 24.0),
        back_btn_pad = s(1.5),
        back_btn_fs = sm(12.0, 12.0),
        back_btn_rad = s(3.0),
        session_title_fs = s(12.0),
        chat_scroll_pad_y = s(4.0),
        chat_scroll_pad_x = s(8.0),
        welcome_pad_y = s(28.0),
        welcome_pad_x = s(16.0),
        welcome_glyph_fs = s(28.0),
        welcome_glyph_mb = s(8.0),
        welcome_title_fs = s(16.0),
        welcome_title_mb = s(2.0),
        welcome_sub_fs = s(12.0),
        welcome_sub_mb = s(10.0),
        welcome_pill_fs = s(12.0),
        welcome_pill_pad_y = s(4.0),
        welcome_pill_pad_x = s(10.0),
        welcome_pill_rad = s(5.0),
        welcome_pill_h = sm(24.0, 24.0),
        card_user_pad_y = s(6.0),
        card_user_pad_x = s(10.0),
        card_user_rad = s(6.0),
        card_user_mb = s(8.0),
        card_user_badge_fs = s(11.0),
        card_user_idx_fs = s(10.0),
        card_asst_pad_y = s(2.0),
        card_asst_mb = s(8.0),
        asst_avatar_fs = s(12.0),
        asst_avatar_mr = s(4.0),
        asst_badge_fs = s(12.5),
        metrics_pill_fs = s(10.0),
        metrics_pill_pad_y = s(1.5),
        metrics_pill_pad_x = s(5.0),
        metrics_pill_rad = s(3.0),
        metrics_pill_ml = s(4.0),
        asst_tt_fs = s(11.5),
        asst_tt_pad_y = s(1.5),
        asst_tt_pad_x = s(5.0),
        asst_tt_rad = s(3.0),
        disc_m_top = s(3.0),
        disc_m_bot = s(5.0),
        disc_rad = s(5.0),
        disc_pad_y = s(3.0),
        disc_pad_x = s(8.0),
        disc_title_fs = s(11.5),
        disc_title_pad_y = s(2.0),
        disc_title_pad_x = s(4.0),
        steps_mt = s(2.0),
        steps_py = s(2.0),
        step_card_pad_y = s(4.0),
        step_card_pad_x = s(8.0),
        step_card_my = s(1.5),
        step_card_rad = s(4.0),
        step_icon_fs = s(11.5),
        step_icon_mr = s(6.0),
        step_label_fs = s(12.0),
        step_chip_fs = s(11.5),
        step_chip_pad_y = s(1.5),
        step_chip_pad_x = s(6.0),
        step_chip_rad = s(3.0),
        step_chip_ml = s(4.0),
        step_status_fs = s(10.0),
        step_status_pad_y = s(1.5),
        step_status_pad_x = s(6.0),
        step_status_rad = s(3.0),
        tool_card_rad = s(5.0),
        tool_card_my = s(2.5),
        tool_card_pad_y = s(5.0),
        tool_card_pad_x = s(9.0),
        tool_name_fs = s(11.5),
        tool_badge_fs = s(9.5),
        tool_badge_rad = s(3.0),
        tool_badge_pad_y = s(1.5),
        tool_badge_pad_x = s(5.0),
        tool_args_mt = s(2.0),
        tool_args_fs = s(11.0),
        tool_args_rad = s(3.0),
        tool_args_pad_y = s(4.0),
        tool_args_pad_x = s(7.0),
        tool_out_title_fs = s(11.0),
        tool_out_mt = s(2.0),
        tool_out_fs = s(11.0),
        tool_out_rad = s(3.0),
        tool_out_pad_y = s(4.0),
        tool_out_pad_x = s(7.0),
        feedback_mt = s(4.0),
        feedback_pt = s(2.0),
        feedback_btn_size = sm(24.0, 24.0),
        feedback_btn_pad_y = s(2.0),
        feedback_btn_pad_x = s(5.0),
        feedback_btn_rad = s(3.0),
        feedback_btn_fs = s(11.5),
        scroll_btn_size = sm(32.0, 32.0),
        scroll_btn_rad = sm(16.0, 16.0),
        scroll_btn_fs = s(14.0),
        comp_container_rad = s(6.0),
        comp_container_pad_y = s(6.0),
        comp_container_pad_x = s(9.0),
        comp_container_my_top = s(4.0),
        comp_container_mx = s(8.0),
        comp_container_my_bot = s(2.0),
        comp_chips_mb = s(3.0),
        comp_chip_rad = s(10.0),
        comp_chip_pad_y = s(2.0),
        comp_chip_pad_x = s(7.0),
        comp_chip_mr = s(4.0),
        comp_chip_mb = s(2.0),
        comp_chip_icon_fs = s(10.5),
        comp_chip_icon_mr = s(3.0),
        comp_chip_text_fs = s(11.0),
        comp_chip_del_size = sm(18.0, 18.0),
        comp_chip_del_rad = sm(9.0, 9.0),
        comp_chip_del_ml = s(3.0),
        comp_chip_del_fs = s(10.0),
        comp_tv_pad_y = s(2.0),
        comp_tv_min_h = sm(32.0, 32.0),
        comp_toolbar_mt = s(4.0),
        comp_toolbar_pt = s(2.0),
        comp_pill_rad = s(4.0),
        comp_pill_pad_y = sm(2.5, 2.5),
        comp_pill_pad_x = s(7.0),
        comp_pill_fs = s(11.0),
        comp_pill_mr = s(3.0),
        comp_pill_h = sm(24.0, 24.0),
        comp_icon_btn_size = sm(24.0, 24.0),
        comp_icon_btn_pad = s(2.0),
        comp_icon_btn_rad = s(3.0),
        comp_icon_btn_fs = s(12.5),
        comp_icon_btn_mr = s(3.0),
        comp_send_btn_size = sm(26.0, 26.0),
        comp_send_btn_rad = sm(13.0, 13.0),
        comp_send_btn_fs = s(12.5),
        comp_send_stop_fs = s(11.5),
        status_pad_top = s(3.0),
        status_pad_side = s(10.0),
        status_pad_bot = s(5.0),
        status_label_fs = s(11.5),
        status_btn_rad = s(4.0),
        status_btn_pad_y = s(2.0),
        status_btn_pad_x = s(5.0),
        status_btn_fs = s(11.5),
        status_btn_h = sm(24.0, 24.0),
        status_sep_fs = s(10.5),
        status_sep_mx = s(1.0),
        status_icon_fs = s(11.5),
        status_icon_mr = s(2.0),
        status_dot_size = s(7.5),
        status_dot_rad = s(4.0),
        pop_contents_rad = s(6.0),
        pop_contents_pad = s(5.0),
        pop_title_fs = s(11.0),
        pop_title_pad_top = s(3.0),
        pop_title_pad_x = s(7.0),
        pop_title_pad_bot = s(4.0),
        pop_title_mb = s(2.0),
        pop_item_pad_y = s(4.0),
        pop_item_pad_x = s(9.0),
        pop_item_rad = s(4.0),
        pop_item_fs = s(12.0),
        pop_item_mb = s(1.0),
        pop_item_min_h = sm(26.0, 26.0),
        pop_desc_fs = s(10.5),
        pop_check_pad_y = s(5.0),
        pop_check_pad_x = s(9.0),
        pop_check_fs = s(12.0),
        pop_check_min_h = sm(26.0, 26.0),
        pop_check_box_size = sm(15.0, 15.0),
        pop_check_box_rad = s(4.0),
        prompt_row_pad_y = s(3.5),
        prompt_row_pad_x = s(6.0),
        prompt_row_rad = s(6.0),
        prompt_pill_fs = s(11.0),
        prompt_pill_rad = s(4.0),
        prompt_pill_pad_y = s(1.5),
        prompt_pill_pad_x = s(5.0),
        prompt_pill_mr = s(6.0),
        prompt_snip_fs = s(12.0),
        prompt_del_size = sm(24.0, 24.0),
        prompt_del_pad_y = s(2.0),
        prompt_del_pad_x = s(4.0),
        prompt_del_rad = s(4.0),
        prompt_del_fs = s(11.5),
        prompt_del_ml = s(4.0),
        confirm_rad = s(8.0),
        confirm_pad_y = s(12.0),
        confirm_pad_x = s(14.0),
        confirm_title_fs = s(13.0),
        confirm_title_mb = s(3.0),
        confirm_desc_fs = s(12.0),
        confirm_desc_mb = s(10.0),
        confirm_btn_fs = s(12.0),
        confirm_btn_pad_y = s(4.5),
        confirm_btn_pad_x = s(12.0),
        confirm_btn_rad = s(5.0),
        confirm_btn_h = sm(26.0, 26.0),
        code_box_rad = s(5.0),
        code_box_my = s(4.0),
        code_hdr_pad_y = s(3.0),
        code_hdr_pad_x = s(8.0),
        code_hdr_rad = s(4.0),
        code_lang_fs = s(10.5),
        code_copy_fs = s(10.5),
        code_copy_pad_y = s(2.0),
        code_copy_pad_x = s(6.0),
        code_copy_rad = s(3.0),
        code_copy_min_h = sm(22.0, 22.0),
        code_body_pt = s(5.0),
        code_body_px = s(8.0),
        code_body_pb = s(6.0),
        code_body_fs = s(11.5),
        tbl_card_rad = s(5.0),
        tbl_card_my = s(4.0),
        tbl_hdr_pad_y = s(3.5),
        tbl_hdr_pad_x = s(8.0),
        tbl_hdr_fs = s(11.5),
        tbl_cell_pad_y = s(3.0),
        tbl_cell_pad_x = s(8.0),
        tbl_cell_fs = s(11.5),
        tbl_tt_fs = s(11.0),
        tbl_tt_rad = s(3.0),
        tbl_tt_pad_y = s(1.5),
        tbl_tt_pad_x = s(4.0),
        head_btn_size = sm(44.0, 44.0),
        head_btn_rad = sm(22.0, 22.0),
        head_icon_fs = s(18.0),
        head_dot_size = s(8.0),
        head_dot_rad = s(4.0)
    )
}

/// Applies the scaled CSS to a GTK CssProvider.
pub fn apply_ui_scale(provider: &gtk4::CssProvider, scale_percent: u32, base_font_size: u32) {
    let css = generate_scale_css(scale_percent, base_font_size);
    provider.load_from_data(&css);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DEFAULT_FONT_SIZE, DEFAULT_UI_SCALE};

    #[test]
    fn test_generate_scale_css_contains_core_elements() {
        let css = generate_scale_css(DEFAULT_UI_SCALE, DEFAULT_FONT_SIZE);
        assert!(css.contains(".panel-header"));
        assert!(css.contains(".header-icon-btn"));
        assert!(css.contains(".card-user"));
        assert!(css.contains(".card-assistant"));
        assert!(css.contains(".composer-container"));
        assert!(css.contains(".composer-pill-btn"));
        assert!(css.contains(".code-block-content"));
        assert!(css.contains(".table-cell-text"));
        assert!(css.contains(".clean-popover-item"));
    }

    #[test]
    fn test_scale_factor_increases_dimensions() {
        let css_100 = generate_scale_css(100, 13);
        let css_150 = generate_scale_css(150, 13);
        assert!(css_100.contains("13px"));
        assert!(css_150.contains("19.5px"));
    }

    #[test]
    fn test_scale_bounds_clamping() {
        let css_low = generate_scale_css(10, 13);
        let css_high = generate_scale_css(1000, 13);
        assert!(css_low.contains("80%"));
        assert!(css_high.contains("250%"));
    }

    #[test]
    fn test_generate_scale_css_all_components_scale_gracefully() {
        let css = generate_scale_css(130, 17);
        // Table components
        assert!(css.contains(".markdown-table-card"));
        assert!(css.contains(".table-cell-header"));
        assert!(css.contains(".table-header-text"));
        assert!(css.contains(".table-cell"));
        assert!(css.contains(".table-cell-text"));
        // Code block components
        assert!(css.contains(".code-block-container"));
        assert!(css.contains(".code-block-header"));
        assert!(css.contains(".code-block-copy-btn"));
        assert!(css.contains(".code-block-content"));
        // Thinking / progress disclosure
        assert!(css.contains(".progress-steps-disclosure"));
        assert!(css.contains(".step-item-card"));
        assert!(css.contains(".step-item-file-chip"));
        assert!(css.contains(".step-item-status"));
        // Context chips
        assert!(css.contains(".composer-chip"));
        assert!(css.contains(".composer-chip-icon"));
        assert!(css.contains(".composer-chip-text"));
        assert!(css.contains(".composer-chip-del"));
        // Composer controls
        assert!(css.contains(".composer-container"));
        assert!(css.contains(".composer-text-view"));
        assert!(css.contains(".composer-toolbar"));
        assert!(css.contains(".composer-pill-btn"));
        assert!(css.contains(".composer-icon-btn"));
        assert!(css.contains(".composer-send-btn"));
        // Chat head and actions
        assert!(css.contains(".chat-head-btn"));
        assert!(css.contains(".btn-scroll-bottom"));
        assert!(css.contains(".confirm-popover"));
    }

    #[test]
    fn test_scale_presets_effective_text_proportions() {
        for preset in crate::settings::UI_SCALE_PRESETS {
            let css = generate_scale_css(preset.scale, preset.font_size);
            let factor = preset.scale as f64 / 100.0;
            let expected_fs = ((preset.font_size as f64) * factor * 10.0).round() / 10.0;
            // Verify font size is in reasonable bounds (< 25px for standard presets) and not double scaled
            assert!(expected_fs <= 25.0);
            assert!(css.contains(&format!("font-size: {}px;", expected_fs)));
        }
    }
}
