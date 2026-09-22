# Implementation Plan — spec-viewer-fs-event-targeted-update

## 정의
fs 이벤트를 표시-대상 관련성으로 걸러 관련 변화만, 해당 파일/폴더만 갱신함으로써, 활성 워크스페이스에서 유휴 CPU를 `--no-watch` 수준으로 되돌리는 수정을 재현 테스트로 고정하고 구현하는 계획이다.

- [ ] 1. 결함 재현/불변 테스트 작성
- [ ] 1.1 관련성 게이트 재현 테스트: 배치 경로 목록의 표시-대상 관련성을 판정하는 게이트 단위를 추출하고, 무관 배치(`.md` 아닌 파일 수정)가 갱신을 유발하지 않아야 함을 어서션하는 테스트 추가 — 현재 "항상 갱신" 동작으로 실패
  - DONE: `fs_event_gate_drops_irrelevant_batches` 테스트가 수정 전 현재 코드에서 실패함을 확인
  - _Requirements: 1.1, 1.2, 2.2_
  - _Difficulty: mid_
  - _Boundary: app_

- [ ] 1.2 (P) 불변 동작 고정 테스트: 표시 대상 변화(생성·수정·삭제)의 갱신 정확성, 선택·펼침 유지, `r` 수동 갱신, `--all` 트리 캐시 무효화 정확성이 현재 경로에서 통과함을 확인·보강 (수정 전후 모두 통과)
  - DONE: 갱신 정확성·선택 유지·수동 갱신·캐시 무효화 어서션 테스트가 현재 코드에서 통과
  - _Requirements: 3.1, 3.2, 3.3, 3.4_
  - _Difficulty: low_
  - _Boundary: app, spec/fs_tree_

- [ ] 2. 수정 구현
- [ ] 2.1 관련성 게이트 구현: `.md`/`.markdown` 파일 이벤트, 디렉터리 생성·삭제, 판별 불가 배치만 갱신 대상으로 인정하고 나머지는 폐기
  - DONE: 1.1 재현 테스트가 통과로 전환; 게이트가 무관 배치를 갱신 없이 폐기함
  - _Requirements: 2.2_
  - _Difficulty: low_
  - _Boundary: app_

- [ ] 2.2 (P) `--all` 경로 패치 함수군: 파일 추가(정렬 위치 삽입+조상 보충), 파일 삭제(엔트리 제거+조상 정리), 디렉터리 추가(하위 트리만 스캔·삽입), 디렉터리 삭제(하위 전체 제거+조상 정리)의 순수 패치 함수를 `FsTree` 불변식(경로 정렬 순, kept-dirs, depth) 안에서 구현하고 임시 디렉터리 테스트로 고정
  - DONE: 패치 적용 결과가 동일 상태의 전체 재스캔 결과와 동일함을 비교 어서션하는 테스트 전부 통과
  - _Requirements: 2.3, 3.1, 3.4_
  - _Difficulty: high_
  - _Boundary: spec/fs_tree_
  - _Depends: 1.1_

- [ ] 2.3 (P) `.kiro` 슬라이스 갱신: 경로→스펙/steering 매핑으로 영향받은 슬라이스만 재판독·재구성, 현재 문서는 해당 경로일 때만 재판독; 매핑 실패·새 스펙 디렉터리 등은 전체 재구동으로 폴백
  - DONE: 영향 슬라이스만 갱신되고 매핑 실패 형태가 전체 재구동으로 처리됨을 어서션하는 테스트 통과
  - _Requirements: 2.3, 3.1, 3.3_
  - _Difficulty: mid_
  - _Boundary: app/loader_
  - _Depends: 1.1_

- [ ] 2.4 통합: 게이트→패치/폴백을 `spec-viewer-kiro-mode-freeze`가 도입한 백그라운드 계산/적용 분리 위에 연결 — 게이트 통과 배치만 계산 예약
  - DONE: 무관 배치가 계산 예약 없이 폐기되고, 관련 배치가 해당 항목 갱신으로 이어짐을 임시 디렉터리 시나리오 테스트로 확인
  - _Requirements: 2.1, 2.2, 2.3_
  - _Difficulty: mid_
  - _Boundary: app, main_
  - _Depends: 2.1, 2.2, 2.3, kiro-mode-freeze 2.2_

- [ ] 3. 전체 검증
- [ ] 3.1 회귀 스위트 + 실기동 CPU 수렴 확인
  - DONE: `cargo test -p spec-viewer` 전체 통과; `m --all ~/w` 실기동에서 유휴 CPU가 `--no-watch` 수준(실측 41% 대비 수 % 이내)으로 수렴함을 5초 샘플로 확인
  - _Requirements: 2.1, 2.2, 2.3, 3.1, 3.2, 3.3, 3.4_
  - _Difficulty: low_
  - _Depends: 2.4_
