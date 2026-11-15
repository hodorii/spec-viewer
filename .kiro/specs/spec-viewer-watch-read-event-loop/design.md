# Design - spec-viewer-watch-read-event-loop

## 정의
`watch` 콜백이 읽기 전용 접근 이벤트를 변경으로 전달하지 않게 하여, 재스캔이 스스로 다음 재스캔을 부르는 되먹임을 끊는 최소 변경이다.

## 원인 (Root Cause)
- notify 8.2.1 의 inotify 백엔드는 감시 마스크에 `WatchMask::OPEN` 을 포함한다(`inotify.rs` 427~434행). 그래서 파일 열기와 디렉터리 읽기 접근이 `EventKind::Access(..)` 로 보고된다.
- `src/watch/mod.rs` 의 `watch::start_with_cache` 콜백은 이벤트 종류와 무관하게 모든 경로를 `FsEvent` 로 전달했다.
- 재스캔은 디렉터리를 연다. 그 열기가 `Access` 이벤트가 되어 `FsEvent` 로 전달되고, 이것이 다음 재스캔을 부른다. 이 되먹임이 1.1~1.3 의 원인이다. 재현 절차의 OPEN 13~14회(`--no-watch` 는 1회)가 이를 뒷받침한다.

## 수정 방식
- 콜백에서 읽기 전용 접근을 거른다: `EventKind::Access` 중 쓰기 완료(`Close(Write)`)를 제외한 것은 전달하지 않는다. 판정은 `is_read_only_access` 한 곳에 둔다.
- 쓰기 완료(`Close(Write)`)와 생성/수정/삭제/rename 은 지금처럼 전달한다. 외부 편집 감지(3.1, 3.3)가 유지된다.
- 기각한 대안: 감시 마스크에서 `OPEN` 을 빼는 방안. notify 내부 설정이라 이 크레이트에서 바꿀 수 없고, 바꾸려면 크레이트를 수정해야 한다.

## 검증 속성
- (a) 결함 재현: 신규 테스트 `watch::tests::reading_files_and_listing_folders_is_not_reported_as_a_change` 가 파일 읽기와 폴더 목록 읽기가 변경으로 보고되지 않아야 한다고 단언하며, 수정 전에는 실패했다(RED). `Recursive` 와 `OpenedDirs` 두 범위 모두에서 검사한다(1.1, 1.2).
- (b) 기대 동작: 같은 테스트가 수정 후 통과한다(2.1, 2.2). 실물에서 유휴 5초간 디렉터리 OPEN 이 14회에서 1회로, 유휴 CPU 가 0 tick 으로 줄고, tasks-view 에서 `l` 후 Enter 가 requirements.md 로 이동함을 확인했다(2.3).
- (c) 불변 동작: 기존 watch 테스트 16개가 통과한다(3.1~3.3). 실물에서 열린 문서를 외부에서 편집하면 2.5초 안에 화면에 반영됨을 확인했다. `--no-watch` 경로는 건드리지 않는다.

## 영향 범위
- 변경 파일: `src/watch/mod.rs` 만(이벤트 필터 `is_read_only_access` 와 그 테스트).
- 재스캔 로직, 디바운스, 감시 범위, app 의 이벤트 처리는 바꾸지 않으며, `Watch::start` 의 시그니처와 `FsEvent` 형태도 그대로여서 Boundary Commitment 위반이 없다.
