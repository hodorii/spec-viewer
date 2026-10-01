# Implementation Plan — spec-viewer-mouse-scroll-freeze

## 정의
마우스 모션 이벤트의 원천 차단과 입력 버스트 병합으로, 마우스 스크롤이 키보드 스크롤과 동일하게 즉시 반응하게 하는 수정을 재현 테스트로 고정하고 구현하는 최소 계획이다.

- [x] 1. 결함 재현/불변 테스트 작성
- [x] 1.1 활성화 시퀀스 재현 테스트: 마우스 캡처 활성화 시퀀스 생성부를 직렬화 가능한 단위로 추출하고, 그 출력이 모션 추적(`?1003h`)을 포함하지 않아야 함을 어서션하는 테스트를 추가 — 현재 시퀀스(1003h 포함)로 실패
  - DONE: `enable_mouse_capture_without_motion_tracking`로 추출, 실제 fd 1 캡처 테스트(`enable_mouse_capture_without_motion_tracking_omits_motion_and_rxvt_modes`)로 `?1003h`/`?1015h` 부재와 `?1000h`/`?1002h`/`?1006h` 보존을 확인.
  - _Requirements: 1.1, 2.2_
  - _Difficulty: low_
  - _Boundary: main_

- [x] 1.2 (P) 불변 동작 고정 테스트: 모션 이벤트가 어떤 상태도 바꾸지 않음, 휠 이벤트가 트리 3행 선택 이동, 클릭·드래그·자동스크롤 제스처가 종전대로 동작함을 어서션하는 테스트 추가 (수정 전후 모두 통과)
  - DONE: 신규 `pointer_moved_without_a_button_changes_nothing`(모션 무변화) + 기존 `wheel_over_doc_scrolls_it_three_rows_regardless_of_focus`/`wheel_over_tree_calls_key_down_the_right_number_of_times_regardless_of_focus`(휠 3행) + 기존 클릭/드래그/자동스크롤 제스처 테스트 전부 통과.
  - _Requirements: 3.1, 3.2_
  - _Difficulty: low_
  - _Boundary: app/mouse_

- [x] 2. 수정 구현
- [x] 2.1 모션 추적 해제: 활성화 시퀀스를 누름/뗌+버튼 드래그+SGR(`?1000h`+`?1002h`+`?1006h`)로 교체하고 복원 경로 정합성 확인
  - DONE: `main::enable_mouse_capture_without_motion_tracking`을 `main()` 시작과 `run_editor`의 에디터 복귀 재활성화 두 곳 모두에 적용(design.md가 명시하지 않은 두 번째 호출부도 같은 버그 메커니즘이라 함께 수정 — 안 했으면 에디터 종료마다 모션 추적이 되살아남). 1.1 재현 테스트가 통과로 전환됨.
  - _Requirements: 2.2, 3.1, 3.2_
  - _Difficulty: low_
  - _Boundary: main_

- [x] 2.2 입력 버스트 드레인: 이벤트 1개 처리 후 대기 중 이벤트를 전부 소진(리듀서만)하고 렌더는 1회 수행하도록 루프 구조 변경; `Control::Quit`은 드레인 중 즉시 탈출. 버스트 N개 → 렌더 1회, Quit 포함 이벤트 무손실을 카운팅 백엔드 단위 테스트로 추가
  - DONE: 이번 재개 이전, 2026-09-21 커밋 `f869180`("fix: coalesce input events per draw to stop scroll lag on diagram docs")으로 이미 구현되어 있었음을 확인 — `run_loop`가 이벤트 1개당 리듀서만 반복 수행 후 드레인 종료 시 렌더 1회, `Control::Quit`은 즉시 탈출. 이번 재개 작업에서 코드 변경 없이 재확인만 함.
  - _Requirements: 2.1, 3.3_
  - _Difficulty: mid_
  - _Boundary: main_

- [x] 3. 전체 검증
- [x] 3.1 회귀 스위트 + 실기동 확인
  - DONE: 전체 워크스페이스 `cargo test` 통과(기존에 문서화된 타이밍 플레이키 테스트 2종 격리 재확인 외 전부 그린). 입력 버스트 드레인(2.2)은 이미 실기동 검증이 끝난 기존 수정이고, 이번에 새로 구현한 모션 추적 해제(2.1)는 자동 테스트(fd 1 바이트 캡처)로 직접 어서션했으며 실제 포인터 모션은 터미널 이벤트 레벨이라 pty 스모크로는 추가 증거를 얻을 수 없어 생략.
  - _Requirements: 2.1, 2.2, 3.1, 3.2, 3.3_
  - _Difficulty: low_
  - _Depends: 2.1, 2.2_
