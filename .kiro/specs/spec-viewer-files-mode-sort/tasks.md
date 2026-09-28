# Implementation Plan — spec-viewer-files-mode-sort

## 정의
spec-viewer 구현자를 위해 design.md 컴포넌트 경계(`spec::fs_tree`/`spec::sort`/`app::mod`/`main`/`ui::tree_panel`)를 따라 관찰 가능한 DONE 상태 단위로 분해한 구현 계획이다.

- [x] 1. spec 모듈: 수정시각 데이터 + 형제 단위 재정렬 + Files용 정렬 키 정규화
  - DONE: `FsTree::scan`이 각 항목의 파일시스템 수정시각을 함께 읽어 `FsEntry.modified`에 채우는 것(조회 실패 시 가장 오래된 값으로 대체), `FsTree::sort_entries(key)`가 트리의 부모-자식 포함 관계는 그대로 두고 각 폴더 레벨 안의 형제 항목만 이름 오름차순 또는 수정시각 내림차순으로 재배열하는 것, `SortKey::for_files`가 phase/진행률을 이름으로 접고 이름/최근 갱신은 그대로 통과시키는 것, `SortKey::cycle_for_files`가 이름↔최근 갱신 2단으로만 순환하는 것을 단위 테스트로 확인. 기존 `FsTree::scan`/`SortKey::cycle`(4단)의 동작과 결과는 전혀 바뀌지 않음을 회귀 테스트로 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.4, 2.1_
  - _Boundary: spec::fs_tree (FsEntry, FsTree::sort_entries), spec::sort (SortKey::for_files, cycle_for_files)_
  - _Difficulty: high_
  - _BizProcess: BP-FILES-SORT.L2-D1_

- [x] 2. 배선과 표시
- [x] 2.1 (P) app::mod: 정렬 순환·재스캔이 실제 재정렬로 이어지게 배선
  - DONE: `--all` 모드에서 정렬 순환 키를 누르면 `cycle_sort`가 `SortKey::cycle_for_files`로 키를 바꾸고 `FsTree::sort_entries`로 실제 형제 순서를 바꾸는 것, `.kiro`/spec-kit 모드에서는 기존과 동일하게(4단 순환 또는 무동작) 동작하는 것, `--all` 모드에서 파일 변경 감지로 재스캔될 때 그 시점의 정렬 키가 다시 적용되는 것(임의로 이름순으로 되돌아가지 않음), 정렬 키를 바꿔도 트리의 펼침·선택 상태는 그대로인 것을 단위/통합 테스트로 확인.
  - _Requirements: 1.1, 1.2, 1.3, 2.1, 2.2, 2.3, 4.1, 4.2_
  - _Boundary: app::mod (cycle_sort, resync)_
  - _Depends: 1_
  - _Difficulty: mid_
  - _BizProcess: BP-FILES-SORT.L2-D1_
- [x] 2.2 (P) main: 시작·모드 전환 시 정렬 키 정규화와 초기 적용
  - DONE: `--all`로 시작할 때 `--sort`로 phase/진행률을 지정해도 오류 없이 이름 정렬로 시작하는 것, 이름/최근 갱신을 지정하면 그 정렬이 실제로 첫 렌더에 반영되는 것, 실행 중 스펙 모드에서 전체 모드로 전환할 때(모드 전환 키)도 그 시점의 정렬 키가 새 트리에 적용되는 것을 통합 테스트로 확인.
  - _Requirements: 1.4, 2.4_
  - _Boundary: main (main, handle_switch_mode)_
  - _Depends: 1_
  - _Difficulty: mid_
  - _BizProcess: BP-FILES-SORT.L2-D1_
- [x] 2.3 (P) ui::tree_panel: Files 모드 패널 제목에 정렬 키 표시
  - DONE: `--all` 모드의 트리 패널 제목이 `.kiro` 모드의 `"Specs [이름]"`과 같은 형식으로 `"Files [이름]"`/`"Files [최근 갱신]"`을 실제 렌더된 프레임에 보여주는 것을 실물 렌더 테스트로 확인.
  - _Requirements: 3.1_
  - _Boundary: ui::tree_panel (render_files)_
  - _Depends: 1_
  - _Difficulty: low_
  - _BizProcess: BP-FILES-SORT.L2-D2_

- [x] 3. 통합과 회귀 확인
- [x] 3.1 전체 컴파일과 기존 기능 무회귀 확인
  - DONE: 워크스페이스 전체가 컴파일되고(`cargo test --no-run` 포함) `.kiro`/spec-kit 모드의 정렬 테스트, `spec-viewer-kiro-folder-groups`의 그룹 순서 테스트가 모두 그대로 통과하는 것을 확인. `app::loader::read_groups`가 `FsTree::sort_entries`를 호출하지 않음을 코드로 재확인.
  - _Requirements: 4.1, 4.2, 4.3_
  - _Depends: 2.1, 2.2, 2.3_
  - _Difficulty: low_
  - _BizProcess: BP-FILES-SORT.L2-D4_
- [x] 3.2 실물 렌더 E2E 확인
  - DONE: 실제 임시 디렉터리(하위 폴더 포함, 파일마다 수정시각이 다르게)로 `--all` 모드를 열어, 실제 `s` 키 입력으로 이름→최근 수정→이름 순으로 순환하며 매 단계 실제 렌더된 프레임의 형제 순서가 바뀌는 것, 정렬 중에도 펼침·선택이 유지되는 것, 패널 제목이 함께 바뀌는 것을 확인.
  - _Requirements: 1.1, 1.2, 1.3, 2.1, 2.2, 3.1_
  - _Depends: 3.1_
  - _Difficulty: mid_
  - _BizProcess: BP-FILES-SORT.L1_

- [x] 4. 검증
- [x] 4.1 실제 컴파일된 바이너리 pty 스모크 테스트
  - DONE: 실제 컴파일된 릴리스 바이너리를 `--all` 모드로 pty에서 구동해, `s` 키로 이름↔최근 수정 순환이 실제 화면에 반영되고 패널 제목이 함께 바뀌는 것을 재확인.
  - _Requirements: 1.1, 1.2, 1.3, 3.1_
  - _Depends: 3.2_
  - _Difficulty: low_
