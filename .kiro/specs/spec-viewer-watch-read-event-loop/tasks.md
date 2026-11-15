# Implementation Plan - spec-viewer-watch-read-event-loop

## 정의
spec-viewer 사용자를 위해 유휴 상태의 재스캔 되먹임을 재현 테스트로 고정하고, 읽기 전용 접근 이벤트를 거르는 필터로 제거하는 최소 구현 계획이다.

- [x] 1. 결함 재현 테스트 작성
- [x] 1.1 파일 읽기와 폴더 목록 읽기가 변경으로 보고되지 않아야 한다는 테스트 `reading_files_and_listing_folders_is_not_reported_as_a_change` 추가(`Recursive` 와 `OpenedDirs` 두 범위)
  - DONE: 수정 전 코드에서 이 테스트가 실패(RED)함을 확인.
  - _Requirements: 1.1, 1.2_
  - _Difficulty: low_
  - _Boundary: watch_

- [x] 2. 불변 동작 baseline 확인
- [x] 2.1 (P) 기존 watch 테스트 16개가 수정 전 코드에서 통과함을 확인
  - DONE: 외부 편집/생성/삭제 감지와 쓰기 완료 보고 테스트가 통과한 기록.
  - _Requirements: 3.1, 3.3_
  - _Difficulty: low_
  - _Boundary: watch_

- [x] 3. 읽기 전용 접근 필터 적용
- [x] 3.1 `src/watch/mod.rs` 콜백이 읽기 전용 접근(`EventKind::Access` 중 `Close(Write)` 제외)을 전달하지 않도록 `is_read_only_access` 를 추가하고 적용
  - DONE: 컴파일 통과, 1.1 의 테스트가 통과, 기존 watch 테스트 16개 계속 통과.
  - _Requirements: 2.1, 2.2, 3.3_
  - _Difficulty: low_
  - _Boundary: watch_
  - _Depends: 1.1_

- [x] 4. 전체 검증
- [x] 4.1 회귀 스위트와 실제 실행(real-run) 확인
  - DONE: 전체 테스트 통과; 실물에서 유휴 5초 디렉터리 OPEN 14회에서 1회, 유휴 CPU 5초간 0 tick, 열린 문서를 외부에서 편집하면 2.5초 안에 화면 반영, tasks-view 에서 `l` 후 Enter 가 requirements.md 로 이동(`--no-watch` 와 동일), `--no-watch` 불변 확인.
  - _Requirements: 2.1, 2.2, 2.3, 3.1, 3.2, 3.3_
  - _Difficulty: low_
  - _Depends: 3.1_
