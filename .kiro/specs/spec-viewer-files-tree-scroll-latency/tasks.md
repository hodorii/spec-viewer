# Implementation Plan — spec-viewer-files-tree-scroll-latency

## 정의
spec-viewer 사용자를 위해 `--all` 모드의 대형 `Files` 트리 스크롤 지연을 재현 테스트로 고정하고, `Vec<TreeItem>` 빌드를 캐시해 제거하는 최소 구현 계획이다.

- [x] 1. 결함 재현/캐시 검증 테스트 작성
- [x] 1.1 20,000개 항목 `Files` 트리에서 연속 렌더의 캐시 적중이 재구성보다 유의미하게 빠름을 보이는 단위 테스트 추가 (`files_tree_render_reuses_cached_items_when_unchanged`)
  - DONE: 첫 렌더(캐시 미스) 대비 두 번째 렌더(캐시 히트) 시간이 절반 미만임을 어서션하는 테스트가 통과.
  - _Requirements: 1.1, 1.2, 2.1_
  - _Difficulty: low_
  - _Boundary: ui/tree_panel_

- [x] 2. 캐시 무효화 정확성 테스트
- [x] 2.1 (P) 검색어(하이라이트) 변경 시 캐시가 새 내용을 반영하는지 확인 (`files_tree_render_rebuilds_when_search_matches_change`)
  - DONE: 새로 검색된 항목이 캐시된(하이라이트 없는) 렌더가 아니라 하이라이트된 상태로 그려짐을 확인.
  - _Requirements: 2.2, 3.2_
  - _Difficulty: low_
  - _Boundary: ui/tree_panel_
- [x] 2.2 (P) 엔트리(파일 목록) 변경 시 캐시가 새 내용을 반영하는지 확인 (`files_tree_render_rebuilds_when_entries_change`)
  - DONE: 재스캔으로 늘어난 파일이 다음 렌더에 바로 나타남을 확인.
  - _Requirements: 2.2, 3.1_
  - _Difficulty: low_
  - _Boundary: ui/tree_panel_

- [x] 3. `Vec<TreeItem>` 빌드 캐시 추가
- [x] 3.1 `AppState`에 `files_tree_cache: Option<FilesTreeItemCache>` 필드 추가, `ui::tree_panel::render`/`render_files`가 이를 받아 `entries`/`search_matches` 불변 시 재사용하도록 변경, `ui::mod`의 세 호출부 갱신
  - DONE: 컴파일 통과, 1.1/2.1/2.2의 테스트가 모두 통과.
  - _Requirements: 2.1, 2.2_
  - _Difficulty: low_
  - _Boundary: app, ui/tree_panel, ui/mod_
  - _Depends: 1.1, 2.1, 2.2_

- [x] 4. 전체 검증
- [x] 4.1 회귀 스위트 + 실제 실행(real-run) 확인
  - DONE: `cargo test -p spec-viewer` 전체 통과(409개, 신규 3개 포함); 격리 벤치마크로 20,000개 항목 기준 스텝당 9.07ms → 3.26ms 확인; 실제 릴리스 빌드로 8,000개 `.md` 파일이 있는 디렉터리에서 `m --all <dir>` 기본 실행 후 60회 연속 Down 스크롤이 지연 없이 반영됨을 tmux 실기동으로 확인.
  - _Requirements: 2.1, 3.1, 3.2, 3.3, 3.4_
  - _Difficulty: low_
  - _Depends: 3.1_
