# Implementation Plan — spec-viewer-editor-mode

## 정의
spec-viewer-editor-mode 구현자를 위해 design.md 컴포넌트 경계(app/keymap/main)를 따라 관찰 가능한 DONE 상태 단위로 분해한 구현 계획이다.

- [ ] 1. 리듀서 신호와 편집 대상 판정
- [ ] 1.1 편집 요청 판정과 신호 타입
  - DONE: 문서 패널에 파일이 표시된 상태에서 `Action::Edit` → `Control::EditFile(path)` 반환, 그 외 문서 패널 상태(빈 화면·정의 요약·누락·삭제됨·읽기 오류·메타 오류) 6가지 모두에서 `Control::Continue` + 상태 표시줄에 "편집할 파일이 없습니다" 안내가 뜨는 리듀서 단위 테스트 통과.
  - _Requirements: 1.1, 1.3_
  - _Boundary: app/mod.rs (Action, Control, current_editable_path)_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B1_
- [ ] 1.2 에디터 실패 보고 액션
  - DONE: `Action::EditFailed(msg)`를 리듀서에 넣으면 `state.popup`에 그 메시지가 그대로 담기는 단위 테스트 통과.
  - _Requirements: 4.1, 4.2_
  - _Boundary: app/mod.rs_
  - _Depends: 1.1_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B4_

- [ ] 2. (P) 편집 키 바인딩과 도움말 문구
  - DONE: 도움말 팝업에 `e` 키와 "문서 패널에 표시된 파일을 외부 에디터로 열기" 설명이 다른 키와 같은 형식으로 나열되는 테스트 통과.
  - _Requirements: 6.1_
  - _Boundary: app/keymap.rs_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B6_

- [ ] 3. 에디터 선택과 실행
- [ ] 3.1 (P) 에디터 우선순위 결정 함수
  - DONE: `--editor` 지정됨 / `$VISUAL`만 있음 / `$EDITOR`만 있음 / 아무것도 없음(→`vi`) 4가지 조합 각각에서 `resolve_editor`가 정확한 프로그램+고정 인자 벡터를 반환하고, `"code -w"`처럼 인자가 섞인 값이 공백 기준으로 올바르게 토큰화되는 단위 테스트 통과.
  - _Requirements: 2.1, 2.2, 2.3, 2.4_
  - _Boundary: main.rs (resolve_editor)_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B2_
- [ ] 3.2 (P) `--editor` CLI 옵션
  - DONE: `m --editor <값>` 인자를 clap이 파싱해 `Args.editor`에 그 값이 채워지는 테스트 통과.
  - _Requirements: 2.1_
  - _Boundary: main.rs (Args)_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B2_
- [ ] 3.3 터미널 일시정지/에디터 실행/복귀 시퀀스
  - DONE: 즉시 정상 종료하는 테스트 전용 가짜 에디터 스크립트를 지정해 `run_editor`를 호출하면 `EditOutcome::Reloaded`가 반환되고, 호출 전후로 raw mode·대체화면·마우스 캡처가 대칭적으로(진입 전 상태 기준) 토글되는 것이 확인되는 통합 테스트 통과. 진입 시점에 마우스 캡처가 이미 꺼져 있던 경우 복귀 후에도 켜지지 않는 것까지 같은 테스트로 확인.
  - _Requirements: 1.2, 5.1, 5.2_
  - _Boundary: main.rs (run_editor, EditOutcome)_
  - _Depends: 3.1_
  - _Difficulty: high_
  - _BizProcess: BP-SPEC-EDIT.L2-B3_
- [ ] 3.4 실행 실패·비정상 종료 처리
  - DONE: 존재하지 않는 명령을 에디터로 지정하면 `EditOutcome::Failed(메시지)`가 반환되고, 0이 아닌 종료 코드로 끝나는 가짜 에디터 스크립트에서도 `EditOutcome::Failed(다른 메시지)`가 반환되는 단위 테스트 통과.
  - _Requirements: 4.1, 4.2_
  - _Boundary: main.rs (run_editor)_
  - _Depends: 3.3_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-EDIT.L2-B4_

- [ ] 4. `run_loop`의 `Control::EditFile` 분기 연결
  - DONE: 리듀서가 `Control::EditFile(path)`를 반환하면 `run_loop`가 `run_editor`를 호출하고, `EditOutcome::Failed`면 `Action::EditFailed`를, `Reloaded`이면서 감시가 켜져 있으면(`state.watch == Live`) `Action::Fs(FsEvent{paths:[path]})`를 `step`에 넘기는 통합 테스트(`TestBackend` + 실제 자식 프로세스) 통과. 감시가 꺼져 있으면(`Manual`) 아무 액션도 추가로 보내지 않는 것까지 같은 테스트로 확인.
  - _Requirements: 3.1, 3.2, 3.3, 4.1, 4.2_
  - _Boundary: main.rs (run_loop)_
  - _Depends: 1.2, 2, 3.4_
  - _Difficulty: high_
  - _BizProcess: BP-SPEC-EDIT.L2-B3_

- [ ] 5. E2E 검증
- [ ] 5.1 감시 켜짐 상태에서 저장 후 자동 반영
  - DONE: 임시 `.kiro` 트리 + 가짜 에디터 스크립트로 파일을 실제로 고치고 종료하면, 별도 새로고침 키 입력 없이 문서 패널에 바뀐 내용이 나타나는 E2E 테스트 통과.
  - _Requirements: 3.1_
  - _Depends: 4_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-EDIT.L2-B3_
- [ ] 5.2 `--no-watch` 상태에서 자동 반영 없음
  - DONE: 동일 시나리오를 `--no-watch`로 실행하면 에디터 종료 후에도 문서 패널이 편집 전 그대로이고 상태 표시줄에 수동 새로고침 안내가 보이는 테스트 통과.
  - _Requirements: 3.2_
  - _Depends: 5.1_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B3_
- [ ] 5.3 저장하지 않고 종료
  - DONE: 파일을 바꾸지 않고 종료하는 가짜 에디터 시나리오에서 문서 내용이 편집 전과 동일하게 유지되는 테스트 통과.
  - _Requirements: 3.3_
  - _Depends: 5.1_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-EDIT.L2-B3_
- [ ] 5.4 (P) 진입 전 마우스 강등 상태 유지 회귀
  - DONE: 마우스 캡처가 이미 비활성인 상태로 시작한 세션에서 편집 후 복귀해도 마우스 캡처가 다시 켜지지 않는 것을 프레임 단위로 확인하는 회귀 테스트 통과.
  - _Requirements: 5.1_
  - _Depends: 4_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-EDIT.L2-B5_
