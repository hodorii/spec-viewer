# Implementation Plan — spec-viewer-tree-navigation-modes

## 정의
spec-viewer 구현자를 위해 design.md 컴포넌트 경계(`app`/`app::keymap`/`main`)를 따라 관찰 가능한 DONE 상태 단위로 분해한 구현 계획이다.

- [x] 1. 리듀서 확장
- [x] 1.1 모드 전환 신호 타입
  - DONE: `Action::ToggleSourceMode`가 항상 `Control::SwitchMode`를 반환하는 것, `Action::ApplySourceSwitch{source, root}`가 `state.root`/`state.kiro_root`를 교체하고 `state.tree`/`state.doc`/`state.selection`/`state.search`/`state.tree_search`/`state.popup`을 초기 상태로 리셋하는 것, `Action::SwitchModeFailed(msg)`가 `state.popup`에 그 메시지를 그대로 담는 것을 단위 테스트로 확인.
  - _Requirements: 1.4, 1.6, 1.7_
  - _Boundary: app (Action, Control)_
  - _Difficulty: high_
  - _BizProcess: BP-TREE-NAV.L2-B1_
- [x] 1.2 전체펼치기/전체접기 액션
  - DONE: `Action::ExpandAll`이 (기존 `flatten_tree`를 재사용해) 트리의 모든 폴더성 노드(`Spec`/`SteeringGroup`/`Dir`)를 연다는 것, `Action::CollapseAll`이 `TreeState::close_all()`로 전부 접는다는 것, 둘 다 트리 선택 상태를 바꾸지 않는다는 것, 노드가 하나도 없는 트리에서는 패닉 없이 아무 효과가 없다는 것을 단위 테스트로 확인.
  - _Requirements: 2.1, 2.2, 2.3, 2.4_
  - _Boundary: app (Action)_
  - _Depends: 1.1_
  - _Difficulty: mid_
  - _BizProcess: BP-TREE-NAV.L2-B2_

- [x] 2. (P) 모드 전환/전체펼치기·접기 키 바인딩과 도움말
  - DONE: 도움말 팝업에 `m`(모드 전환)/`o`(전체펼치기)/`c`(전체접기) 키와 그 설명이 다른 키와 같은 형식으로 나열되는 테스트 통과.
  - _Requirements: 4.1_
  - _Boundary: app::keymap_
  - _Difficulty: low_
  - _BizProcess: BP-TREE-NAV.L2-B4_

- [x] 3. `main`의 모드 전환 실행
- [x] 3.1 우선순위 판정 로직 추출과 `resolve_source` 무회귀 리팩터링
  - DONE: `resolve_spec_mode(start) -> Result<(TreeSource, PathBuf), String>`가 `.kiro`/`.specify` 우선순위(더 가까운 마커 승리, 같은 디렉터리 공존 시 `.kiro` 우선)를 정확히 판정하는 것을 단위 테스트로 확인하고, `resolve_source`가 이 함수를 호출하도록 리팩터링된 뒤에도 기존 `resolve_source`/`resolve_startup` 관련 테스트 전체가 무회귀로 통과하는 것을 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.4_
  - _Boundary: main (resolve_spec_mode, resolve_source)_
  - _Depends: 1.1_
  - _Difficulty: mid_
- [x] 3.2 `handle_switch_mode`와 `run_loop` 배선
  - DONE: `Control::SwitchMode`를 받으면 현재 `state.root`가 `Files`면 새로 스캔, `Kiro`/`SpecKit`이면 `resolve_spec_mode`를 호출해 성공 시 파일 감시를 새 대상으로 재시작한 뒤 `Action::ApplySourceSwitch`를, 실패 시 `Action::SwitchModeFailed`를 `step()`으로 디스패치하는 통합 테스트(임시 디렉터리, 실제 `watch` 값 교체 확인) 통과.
  - _Requirements: 1.5_
  - _Boundary: main (handle_switch_mode, run_loop)_
  - _Depends: 3.1, 1.1, 2_
  - _Difficulty: high_
  - _BizProcess: BP-TREE-NAV.L2-B1_

- [x] 4. E2E 검증
- [x] 4.1 모드 전환 왕복 실물 확인
  - DONE: 스펙 모드 → 전체 모드 → 스펙 모드로 실제로 왕복 전환했을 때 각 단계에서 트리·문서 패널·파일 감시가 요구사항대로 리셋/재시작되는 것, 그리고 스펙 모드로 전환 시도 시 마커가 없어 실패하는 케이스(화면 유지 + 안내)를 실물 실행으로 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8_
  - _Depends: 3.2_
  - _Difficulty: mid_
  - _BizProcess: BP-TREE-NAV.L2-B1_
- [x] 4.2 (P) 전체펼치기/전체접기 실물 확인
  - DONE: 실제 렌더된 프레임에서 전체펼치기 후 모든 하위 노드가 보이고, 전체접기 후 최상위만 보이며, 그 사이 선택된 노드가 유지되는 것, 빈 트리에서도 안전한 것을 확인.
  - _Requirements: 2.1, 2.2, 2.3, 2.4_
  - _Depends: 1.2_
  - _Difficulty: low_
  - _BizProcess: BP-TREE-NAV.L2-B2_
- [x] 4.3 (P) 기존 동작 무회귀 확인
  - DONE: 개별 노드 펼침·접힘(Left/Right/Enter/클릭)이 여전히 1단계만 동작하고, `--all` CLI 플래그가 여전히 초기 모드만 지정한다는 것을 기존 회귀 스위트 전체 통과로 확인.
  - _Requirements: 3.1, 3.2_
  - _Depends: 3.2_
  - _Difficulty: low_
  - _BizProcess: BP-TREE-NAV.L2-B3_
- [x] 4.4 (P) 도움말 노출 실물 확인
  - DONE: 실제 렌더된 도움말 팝업 프레임에 `m`/`o`/`c` 키와 설명이 다른 키와 같은 형식으로 보이는 것을 확인.
  - _Requirements: 4.1_
  - _Depends: 2_
  - _Difficulty: low_
  - _BizProcess: BP-TREE-NAV.L2-B4_

- [x] 5. (kiro-verify-completion에서 발견, 사용자 승인 후 범위 확장) 도움말 팝업 스크롤 지원
  - DONE: 실제 pty 스모크 테스트로 도움말 팝업이 스크롤을 지원하지 않아 `m`/`o`/`c`(및 기존 `e`)가 일반 터미널 높이에서 안 보이는 것을 발견 → `Popup::Help`에 선택 행 인덱스 추가, `render_help`를 `render_toc`와 동일한 스크롤 가능 `List`로 변경, `j`/`k`/`Up`/`Down` 순환 스크롤 추가. 실제 키 디스패치·실제 렌더 테스트로 확인.
  - _Requirements: 4.1 (재확인)_
  - _Boundary: app (Popup::Help), ui::popup (render_help)_
  - _Difficulty: low_
  - design.md "Verification Addendum" 참고.

- [x] 6. (v0.6.0 배포 후 사용자 피드백, 공개 전이라 하위호환 부담 없음) 전체펼치기/전체접기 키 통합
  - DONE: `Action::ExpandAll`/`CollapseAll`을 `Action::ToggleExpandAll` 하나로 합치고(방향은 `tree_fully_expanded`로 판정), 키 바인딩을 `o`/`c` 두 개에서 `a` 하나로 교체. 기존 회귀(선택 유지, 빈 트리 안전, 실제 키 디스패치)를 새 액션/키 이름으로 재작성해 통과 확인, 실제 바이너리 pty 스모크 테스트로 왕복 재확인.
  - _Requirements: 2.1~2.4 (표현 갱신)_
  - _Boundary: app (Action, keymap)_
  - _Difficulty: low_
  - design.md "Post-Ship Amendment" 참고.

- [x] 7. (바로 이어진 피드백) 토글 키 `a` → `o` 원복
  - DONE: `keymap.rs`의 토글 바인딩 키만 `a`에서 `o`로 교체(액션 이름·판정 로직 불변). 모든 테스트/문서의 `a` 참조를 `o`로 갱신.
  - _Requirements: 2.1~2.4 (키 표기만)_
  - _Boundary: app::keymap_
  - _Difficulty: low_
  - design.md "Post-Ship Amendment 2" 참고.
