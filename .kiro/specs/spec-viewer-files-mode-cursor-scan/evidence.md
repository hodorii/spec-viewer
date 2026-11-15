# spec-viewer-files-mode-cursor-scan 요구사항 20개 증거표

테스트 이름 접두: `main::tests::` = src/main.rs 의 mod tests, `main::round_trip_tests::` = src/round_trip_tests.rs(신규),
`main::switch_history_invariance_tests::` = src/switch_history_invariance_tests.rs(신규), `app::reducer_tests::` = src/app/mod.rs,
`spec::build_tests::` = src/spec/mod.rs, `spec::fs_tree::tests::`, `app::keymap::tests::`, `ui::popup::tests::`, `ui::tree_panel::tests::`.

| 요구 | 증거 (테스트 이름) | 비고 |
|------|--------------------|------|
| 1.1 | `spec::build_tests::scan_root_of_a_spec_or_its_document_is_the_spec_directory`, `scan_root_of_a_folder_is_itself_and_of_a_file_is_its_parent`, `main::tests::switch_to_files_mode_scans_the_spec_dir_when_the_cursor_is_on_a_spec`, E2E `main::round_trip_tests::space_round_trip_returns_to_the_spec_cursor_with_its_ancestors_expanded_on_screen`(뿌리=스펙 디렉터리, 다른 스펙 미표시) | 단위+통합+E2E |
| 1.2 | `app::reducer_tests::entering_files_mode_from_a_spec_document_selects_that_file`, `the_selected_file_is_revealed_by_opening_its_ancestor_folders`, `a_file_missing_from_the_new_files_tree_leaves_the_default_cursor`, `spec::fs_tree::tests::node_path_for_lists_ancestor_dirs_shallowest_first_then_the_file`, `main::tests::switch_to_files_mode_scans_the_containing_spec_dir_when_the_cursor_is_on_a_doc`, E2E `main::round_trip_tests::a_cursor_in_another_spec_enters_files_mode_on_that_specs_own_file`(렌더 커서 행 확인) | |
| 1.3 | `spec::build_tests::scan_root_of_an_empty_selection_or_a_vanished_spec_is_none`, `main::tests::switch_to_files_mode_falls_back_to_the_start_path_without_a_cursor`, `..._when_the_cursor_spec_is_gone` | |
| 1.4 | `main::tests::switch_to_files_mode_with_an_unreadable_cursor_root_keeps_the_screen_and_names_the_path` | 메시지에 경로 포함, 화면/watch 불변 |
| 2.1 | `main::tests::files_flag_parses_with_the_path_argument_unchanged`, `resolve_source_files_mode_needs_no_kiro_root`, `resolve_source_files_mode_defaults_to_current_directory`, `resolve_source_files_mode_rejects_a_nonexistent_directory`, `resolve_source_files_mode_rejects_an_unreadable_directory` | |
| 2.2 | `main::tests::all_flag_in_any_form_is_an_unsupported_flag_error` | |
| 2.3 | `main::tests::help_text_lists_files_and_never_mentions_all` | |
| 3.1 | `app::reducer_tests::returning_to_the_same_files_root_restores_its_cursor_over_the_spec_cursor_file`(스펙 방향 복원 포함 계열), E2E `main::round_trip_tests::space_round_trip_returns_to_the_spec_cursor_with_its_ancestors_expanded_on_screen`(스펙->파일->스펙, 조상 펼침 렌더 확인), `a_moved_cursor_is_what_the_next_round_trip_restores_in_both_modes` | |
| 3.2 | `app::reducer_tests::returning_to_spec_mode_without_a_remembered_cursor_starts_at_the_default` | |
| 3.3 | `app::reducer_tests::returning_to_the_same_files_root_restores_its_cursor_over_the_spec_cursor_file`, E2E `main::round_trip_tests::files_round_trip_over_the_same_root_restores_a_cursor_inside_a_nested_folder`, `a_moved_cursor_is_what_the_next_round_trip_restores_in_both_modes`(복원 > 1.2 파일 선택) | |
| 3.4 | `app::reducer_tests::a_different_files_root_discards_the_earlier_cursor_and_the_earlier_root_keeps_its_own`, E2E `main::round_trip_tests::a_cursor_in_another_spec_enters_files_mode_on_that_specs_own_file` | 아래 "관찰" 참고 |
| 3.5 | `app::reducer_tests::a_failed_switch_leaves_cursor_folds_doc_and_memory_untouched`, `main::tests::switch_to_files_mode_with_an_unreadable_cursor_root_keeps_the_screen_and_names_the_path` | |
| 4.1 | 계측(아래), CI 단언은 `main::tests::switch_to_files_mode_scans_the_*`(스캔 뿌리 == 커서 파생 경로) | 실측 12~20 ms |
| 5.1 | `main::switch_history_invariance_tests::spec_mode_tree_is_identical_whatever_mode_switches_and_moves_came_before`(40 시드), 기존 `main::e2e_kiro_groups_are_discovered_recursive_and_steering_badge_is_exclusive`(tests/app_flow.rs), 전체 기존 스위트 통과 | |
| 5.2 | E2E `main::round_trip_tests::space_round_trip_returns_to_the_spec_cursor_with_its_ancestors_expanded_on_screen`, `a_cursor_in_another_spec_enters_files_mode_on_that_specs_own_file`: 트리 제목 `Files [` 와 새 뿌리의 파일만 나열, 다른 스펙 미표시, `tree.root` 일치. `main::tests::switch_to_files_mode_scans_*`(state.kiro_root) | 현재 UI 는 뿌리 경로 문자열을 따로 그리지 않으므로 뿌리 내용으로 확인 |
| 5.3 | `main::tests::handle_switch_mode_to_files_applies_the_current_sort_key`, `ui::tree_panel::tests::files_panel_title_shows_the_current_sort_key`, `spec::fs_tree::tests::caps_at_max_depth_six`, `excludes_hidden_files_and_directories`, `respects_gitignore`, tests/app_flow.rs `l2_all_mode_browses_a_plain_markdown_tree_with_fold_select_and_live_edits`, `e2e_all_mode_sort_hotkey_cycles_name_and_updated_and_preserves_fold_and_selection` | 기존 스위트 전체 통과 |
| 6.1 | `app::reducer_tests::mode_switch_and_expand_collapse_keys_reach_their_actions_through_the_real_key_dispatch_path`, 모든 E2E(실제 `Action::Key(Space)`) | |
| 6.2 | `app::keymap::tests::m_key_is_unbound`, `space_is_bound_only_to_toggle_source_mode_and_o_c_stay_single_use` | |
| 6.3 | `ui::popup::tests::help_popup_real_render_shows_mode_switch_and_expand_collapse_keys`(Space 행 표시, m 행 부재), `app::keymap::tests::space_key_resolves_to_toggle_source_mode_action` | |
| 6.4 | `app::reducer_tests::space_in_search_input_is_typed_as_a_space_not_a_mode_switch` | |

## 관찰 (3.4)
구현은 `remembered_tree_positions` 를 (소스 종류, 시작 디렉터리, 스캔 뿌리) 키로 둔다(설계 보정). 따라서 뿌리 A 파일 커서 -> 뿌리 B 진입 -> 다시 A 진입 시 A 의 과거 커서가 복원된다.
요구 3.4 문구("과거 파일 모드 뿌리와 다르면 폐기")를 "직전" 한 번만 기억으로 읽으면 차이가 있으나, 설계 보정이 의도한 키 조회 방식이며 reducer 테스트
`a_different_files_root_discards_the_earlier_cursor_and_the_earlier_root_keeps_its_own` 가 이 동작을 명시적으로 고정한다.

## 4.1 전환 지연 계측
조건: 릴리스 빌드(target/release/m), 마크다운 100개(doc_1..doc_100.md + requirements.md)가 든 스펙 디렉터리(.kiro/specs/demo), tmux 140x40,
Down 으로 스펙 행에 커서 -> `tmux send-keys Space` 부터 화면 캡처 해시가 바뀔 때까지(폴링, 캡처 1회 약 4~7 ms 포함 상한치). XDG_STATE_HOME 은 실행마다 새 디렉터리.
- 1회: 13 ms / 2회: 20 ms / 3회: 12 ms (모두 100 ms 이내). 전환 후 화면: `Files [이름]` + 스펙 디렉터리의 doc_*.md 목록(커서 기준 뿌리).
- 스크립트: scratchpad/space_switch_bench.sh, 임시 디렉터리: scratchpad/bench100

## 독립 검토 후 보정 (2026-11-15)
- 이전 실행에서 디스크에 저장된 커서가 새 세션 첫 전환의 파일 선택(1.2)을 덮어쓰던 결함을 고쳤다. 전환 복원은 이번 세션에 표시된 적 있는 키에만 적용한다(`AppState.trees_shown_this_session`). 증거: `app::reducer_tests::a_position_loaded_from_an_earlier_run_does_not_override_the_cursor_file_on_the_first_switch`, `round_trip_tests::a_position_saved_by_an_earlier_run_does_not_override_the_cursor_file_in_a_new_session`. 집합 검사를 항상 참으로 바꾸는 변이에서 두 테스트가 실패했다.
- 같은 세션의 뿌리 A->B->A 왕복은 A의 커서를 복원한다. 요구 3.4의 "직전 한 번만 기억" 해석과 다르며, 요구를 이 동작에 맞춰 개정할지는 사람이 정한다.
- 요구 5.2("트리 패널에 뿌리 경로를 봄")는 현재 UI에 뿌리 경로 표시가 없어 트리 내용으로만 확인했다. 트리 제목에 뿌리를 표시할지, 요구를 "결정한 뿌리 아래 항목만 보인다"로 개정할지는 사람이 정한다.
- 100개 문서 전환 지연은 구현자 계측 12~20 ms, 독립 검토 계측 16~21 ms(모두 tmux 폴링 포함)로 100 ms 이내이다.
