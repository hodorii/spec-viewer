# Implementation Plan — spec-viewer-kiro-mode-freeze

## 정의
fs 이벤트 갱신을 계산/적용으로 분리해 백그라운드에서 수행함으로써, 파일 변화가 계속 일어나는 저장소에서도 `.kiro` 모드 네비게이션·스크롤이 멈추지 않게 하는 수정을 재현 테스트로 고정하고 구현하는 계획이다.

- [ ] 1. 결함 재현/불변 테스트 작성
- [ ] 1.1 재현 테스트: `Action::Fs` 처리 후 갱신이 아직 적용되지 않았음(상태 무변화)과 예약/완료 적용 경로를 통해 갱신됨을 어서션하는 테스트 추가 — 현재 동기 재판독 즉시 적용으로 실패
  - DONE: `fs_event_defers_resync_and_applies_on_completion` 테스트가 수정 전 현재 코드에서 실패함을 확인
  - _Requirements: 1.1, 2.1_
  - _Difficulty: mid_
  - _Boundary: app_

- [ ] 1.2 (P) 불변 동작 고정 테스트: 갱신 후 최신 내용 반영(생성·수정·삭제), 트리 선택·펼침 유지, `r` 수동 갱신 경로가 현재 구조에서 통과함을 확인·보강 (수정 전후 모두 통과)
  - DONE: 갱신 정확성·선택 유지·수동 갱신 어서션 테스트가 현재 코드에서 통과
  - _Requirements: 3.1, 3.2, 3.3_
  - _Difficulty: low_
  - _Boundary: app_

- [ ] 2. 수정 구현
- [ ] 2.1 resync 계산/적용 분리: 갱신 계산(스냅샷 재판독·빌드·정렬·현재 문서 재판독 — 순수 입력: 루트, 정렬 키, 현재 문서 경로, 패널 폭)과 상태 적용(선택 유지 규칙 포함)을 별도 함수로 분리
  - DONE: 계산이 입력만 받아 결과를 반환하고, 적용이 상태만 바꾸는 순수 구조로 분리됨; 1.1 재현 테스트가 통과로 전환
  - _Requirements: 2.1_
  - _Difficulty: mid_
  - _Boundary: app_

- [ ] 2.2 백그라운드 워커·인바운드 채널 통합: 채널 페이로드를 `Fs(FsEvent) | ResyncDone(갱신 결과)` 열거형으로 통합하고, `Action::Fs`는 진행 중이면 재요청 플래그만 설정, 완료 적용 후 재요청 시 재예약. 수동 갱신 `r`도 같은 경로 사용
  - DONE: 워커 진행 중 도착한 추가 이벤트가 갱신 손실 없이 병합됨을 어서션하는 테스트 통과; 실기동에서 파일 변화 연속 발생 중 네비게이션·스크롤이 멈추지 않음
  - _Requirements: 2.1, 2.2_
  - _Difficulty: high_
  - _Boundary: app, main, watch_
  - _Depends: 2.1_

- [ ] 2.3 `--all` 갱신 경로 동일 워커 적용: 전체 스캔 계산도 같은 계산/적용 분리 위에서 수행됨을 확인·연결
  - DONE: `--all` 모드에서 파일 변화 갱신이 동일한 완료-적용 경로로 동작함을 어서션하는 테스트 통과
  - _Requirements: 3.4_
  - _Difficulty: low_
  - _Boundary: app, main_
  - _Depends: 2.2_

- [ ] 3. 전체 검증
- [ ] 3.1 회귀 스위트 + 실기동 확인
  - DONE: `cargo test -p spec-viewer` 전체 통과; 활성 저장소(파일 반복 수정) 시나리오 실기동에서 네비게이션·스크롤이 즉시 반응함을 확인
  - _Requirements: 2.1, 2.2, 3.1, 3.2, 3.3, 3.4_
  - _Difficulty: low_
  - _Depends: 2.2, 2.3_
