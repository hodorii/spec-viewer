# Implementation Plan — spec-viewer-watch-startup-latency

## 정의
spec-viewer 사용자를 위해 대형 디렉터리를 열 때의 감시 초기화 지연을 재현 테스트로 고정하고, `NoCache`로 교체해 제거하는 최소 구현 계획이다.

- [x] 1. 결함 재현 테스트 작성
- [x] 1.1 `watch::start`가 감시 루트 아래 파일 수에 비례해 느려짐을 보이는 단위 테스트 추가 (수정 전에는 반드시 실패/임계값 초과)
  - DONE: 수만 개 파일을 가진 스크래치 디렉터리(또는 동등한 수의 파일을 만들어 채운 임시 디렉터리)를 대상으로 `watch::start` 호출 시간을 측정해 "일정 임계값(예: 300ms) 미만" 어서션을 넣은 테스트가, 수정 전 코드에서는 실패함을 확인.
  - _Requirements: 1.1, 1.2_
  - _Difficulty: low_
  - _Boundary: watch_

- [x] 2. 불변 동작 baseline 확인
- [x] 2.1 (P) 기존 `watch` 모듈 테스트가 수정 전 코드에서 통과함을 확인 (변경 없음 확인용 baseline)
  - DONE: `cargo test -p spec-viewer watch::` 전체 통과 기록.
  - _Requirements: 3.1_
  - _Difficulty: low_
  - _Boundary: watch_

- [x] 3. `NoCache`로 교체
- [x] 3.1 `spec-viewer/src/watch/mod.rs`에서 `new_debouncer` 호출을 `new_debouncer_opt::<_, notify::RecommendedWatcher, notify_debouncer_full::NoCache>(.., NoCache::new(), notify::Config::default())`로, `Watch::Live`의 캐시 타입 파라미터를 `RecommendedCache` → `NoCache`로 교체
  - DONE: 컴파일 통과, 1.1의 재현 테스트가 이제 임계값 이내로 통과.
  - _Requirements: 2.1, 2.2_
  - _Difficulty: low_
  - _Boundary: watch_
  - _Depends: 1.1_

- [x] 4. 전체 검증
- [x] 4.1 회귀 스위트 + 실제 실행(real-run) 확인
  - DONE: `cargo test -p spec-viewer` 전체 통과(2.1의 baseline 테스트 포함, 3.1의 재현 테스트 포함); 실제 릴리스 빌드로 대형 디렉터리(`node_modules`/`target` 다수 파일 포함)에서 `m --all <dir>` 기본 실행 시 첫 프레임이 즉시(체감 지연 없이) 그려짐을 tmux 실기동으로 확인.
  - _Requirements: 2.1, 3.1, 3.2, 3.3_
  - _Difficulty: low_
  - _Depends: 3.1_

