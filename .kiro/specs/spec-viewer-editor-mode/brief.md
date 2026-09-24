# Brief: spec-viewer-editor-mode

## Problem
spec-viewer는 읽기 전용 마크다운 뷰어다. 사용자가 문서 패널에서 오탈자·구조상
문제를 발견해도 그 자리에서 고칠 수 없고, 뷰어를 벗어나 경로를 다시 찾아 별도
터미널에서 vi/vim을 열어야 한다 — 스펙을 훑어보며 즉시 손볼 때마다 맥락 전환
비용이 든다.

## Current State
- `app` 리듀서(`src/app/mod.rs`)는 순수 상태 전이 + 읽기 전용 I/O(`loader`)만
  수행한다. `grep -rn "process::Command|EDITOR|VISUAL" src/`엔 production 코드에
  자식 프로세스 실행 선례가 전혀 없다 — 이번이 최초.
- 터미널 소유권(raw mode·대체화면·마우스 캡처)은 오직 `main.rs`가 쥔다
  (`ratatui::init`/`EnableMouseCapture` ~ 루프 ~ `DisableMouseCapture`/
  `ratatui::restore`). 리듀서의 반환값 `Control`은 `Continue|Quit` 두 가지뿐이라
  "터미널을 잠깐 넘겨달라"는 신호를 보낼 수단이 없다.
- 파일워처(`watch::start`)는 이미 살아 있어, 외부 프로세스가 파일을 고쳐도
  `Action::Fs` → `resync` 경로로 자동 재렌더된다(요구사항 7.x 그대로 재사용 가능).
- 본편 `design.md`에 이미 다음 문장들이 있다:
  - Out-of-Scope "Spec Mutation": "`spec.json` 승인 상태 변경 및 `.kiro/` 하위
    파일 쓰기 일체 — `loader`는 읽기 전용이며 쓰기 경로가 없다."
  - `app` 컴포넌트: "영속 상태 없음, 파일 쓰기 없음."
  - `brief.md`(본편) Constraints: "파일 읽기 전용. 어떤 경로로도 `.kiro/` 아래를
    쓰지 않는다."
  이 세 문장 모두 이번 기능과 정면으로 검토가 필요하다 — spec-viewer 프로세스
  자신은 여전히 한 바이트도 쓰지 않지만("무엇이 쓰는가"는 참 그대로), 사용자가
  뷰어 안에서 외부 에디터를 열어 파일을 바꿀 수 있게 되면 "읽기 전용 뷰어"라는
  취지 자체는 명백히 확장된다.

## Desired Outcome
- 문서 패널에서 열람 중인 문서 위에서 키 한 번으로 `$VISUAL`/`$EDITOR`(미설정
  시 `vi`)가 그 파일을 열고, 종료하면 spec-viewer 화면이 raw mode·대체화면·
  마우스 캡처 상태 그대로 복귀한다. 저장된 변경은 기존 파일워처가 그대로
  집어 자동 재렌더한다(수동 새로고침 불필요).

## Approach
**리듀서-신호 / 메인-실행 분리.** `Action::Edit`는 리듀서 안에서 새 `Control`
변형(예: `Control::Suspend(PathBuf)`)만 돌려주고, 실제 raw mode 해제 →
`EnableMouseCapture` 해제 → 대체화면 이탈 → 자식 프로세스 실행+대기 → 역순
복귀는 터미널을 이미 쥐고 있는 `main.rs`의 루프가 전담한다. 리듀서는 여전히
터미널/프로세스를 모르는 순수 함수로 남는다(design.md Key Decision
"Synchronous Core"·계층 분리 원칙 유지). 에디터 선택은 `$VISUAL` → `$EDITOR`
→ `vi` 순(요청이 명시한 "vi/vim 활용"을 최후 보장값으로 고정).

대안으로 검토했으나 기각: (1) 뷰어 자체 인라인 에디터 구현 — "vi/vim 활용"이라는
요청 취지와 배치되고 구현 비용이 훨씬 큼. (2) 리듀서 안에서 직접
`Command::spawn`+대기 — `app`가 `ui`/터미널을 모른다는 기존 계층 분리
(lib.rs 의존 방향)를 깨뜨림.

## Scope
- **In**: 문서 패널에서 열람 중인 파일 1개를 외부 에디터로 열기/복귀, 터미널
  상태 완전 복원, 편집 후 기존 워처 경로로 자동 재렌더, 신규 키 배정(자유 키
  확인됨: `a c e h i l m o p v w x y` — 대문자 다수 포함), 에디터 선택
  우선순위(`$VISUAL`→`$EDITOR`→`vi`), 에디터 미존재/비정상 종료 시 에러 표시,
  README Keys/Flags 표 갱신.
- **Out**: `.kiro/`의 `spec.json` 승인 상태 조작(여전히 kiro 스킬 소유),
  뷰어 자체 인라인 편집기, 동시에 여러 문서 편집, `Definition`(합성 `## 정의`
  뷰) 편집, `--all` 트리 밖 경로 편집.

## Boundary Candidates
- **app**: `Control`에 "터미널을 넘겨야 한다"는 신호 변형 추가(가칭
  `Control::Suspend(PathBuf)`) — 실행 자체는 하지 않고 신호만.
- **main**: 터미널 일시정지/복귀 헬퍼(raw mode·대체화면·마우스 캡처 토글 +
  자식 프로세스 실행/대기), 에디터 탐색(`$VISUAL`/`$EDITOR`/`vi` 폴백).
- **keymap**: 신규 키 1개 추가, 도움말 팝업에 자동 노출(기존 표 기반 생성
  메커니즘 재사용).

## Out of Boundary
- 본편 design.md의 "Spec Mutation"/"파일 읽기 전용" 경계 문장 자체를 폐기하는
  것 — "spec-viewer 프로세스는 쓰지 않는다"는 진술은 그대로 참으로 유지하고,
  "사용자가 명시적으로 외부 에디터를 여는 경로는 이 경계의 의도적 예외"라는
  각주 수준의 개정만 한다.
- 뷰어 내장 diff/undo, 실시간 협업 편집 등 자체 에디터 기능 일체.

## Upstream / Downstream
- **Upstream**: `watch::start`(파일워처, 기존), `app::loader`(재로드 경로,
  기존), crossterm(raw mode/대체화면/마우스 캡처 API, 기존 의존성).
- **Downstream**: 없음 — 터미널 UI 최종 소비 지점.

## Existing Spec Touchpoints
- **Extends**: `spec-viewer`(본편) — `Components and Interfaces > app`
  (Control/Action 확장), `main.rs`(터미널 소유권 로직 확장), `keymap.rs`
  (키 표 확장), `Error Handling`(자식 프로세스 실패 처리 신규 절),
  `Boundary Commitments`(Out-of-Scope 문구 각주 개정).
- **Adjacent**: 없음 — 기존 bugfix 스펙 5개(files-tree-scroll-latency/
  fs-event-targeted-update/kiro-mode-freeze/mouse-scroll-freeze/
  watch-startup-latency)는 전부 성능·freeze 계열이라 이번 기능과 무관.

## Constraints
- Synchronous Core 원칙 유지(리듀서를 비동기화하지 않는다).
- `--no-watch`와 조합 시 편집 후 자동 재렌더가 안 되므로 `r`(수동 새로고침)
  안내가 필요하다.
- 마우스 캡처가 세션 시작 시점에 이미 실패해 강등된 상태(요구 9.8)라면,
  에디터 복귀 시에도 재시도하지 않고 off 상태를 유지한다(기존 강등 정책과
  일관).
- 동시에 여러 spec-viewer 인스턴스가 같은 파일을 열람 중일 때 파일워처
  재렌더 폭주 여부는 이번 스펙의 requirements 단계에서 확정 필요(미해결
  질문).
