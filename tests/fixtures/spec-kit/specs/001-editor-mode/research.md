> 참고: spec-viewer 자체 spec-kit 지원 검증 픽스처. `.kiro/specs/spec-viewer-editor-mode/research.md`를 그대로 가져왔습니다(spec-kit도 같은 이름의 research.md를 쓴다).

# Research & Design Decisions — spec-viewer-editor-mode

## Summary
- **Feature**: `spec-viewer-editor-mode`
- **Discovery Scope**: Extension (기존 `spec-viewer` 앱에 새 리듀서 변형·터미널 시퀀스 추가, 신규 외부 의존성 없음) — `design-discovery-light.md` 적용, full discovery 승격 조건(대규모 아키텍처 변경/외부 서비스 연동/보안 민감/성능 critical/미검증 의존성) 해당 없음.
- **Key Findings**:
  - 터미널 소유권은 오직 `main.rs`가 쥐고 있고(`ratatui::init`/`restore`, 마우스 캡처 on/off), 리듀서(`app::update`)는 `Control::Continue|Quit` 두 가지만 돌려줄 수 있어 "터미널을 잠깐 넘겨달라"는 신호를 보낼 수단이 없다 — 신호용 `Control` 변형 하나 추가로 해결.
  - `src/app/keymap.rs`의 `KeyCode::Char` 전수 조사 결과 자유 키: `a c e h i l m o p v w x y`(대문자 포함) — `e`(edit)로 확정.
  - 기존 파일워처(`watch::start`)와 `Action::Fs` 분류 경로가 이미 "경로 하나가 바뀌면 그 문서만 재렌더"를 처리하므로, 에디터 종료 후 이 경로를 그대로 재사용하면 새 재로드 로직이 필요 없다.
  - `$EDITOR`/`$VISUAL` 관례상 값에 인자가 포함될 수 있음(예: `EDITOR="code -w"`) — 공백 분리 없이 통째로 실행 파일명 취급하면 실패한다.
  - production 코드에 자식 프로세스 실행 선례가 전무(`grep -rn "process::Command" src/`) — 이번이 최초 도입.

## Research Log

### 터미널 일시정지/복귀 지점
- **Context**: 에디터를 실행하려면 raw mode·대체화면·마우스 캡처를 내렸다가 되돌려야 하는데, 그 상태를 누가 쥐고 있는지 확인 필요.
- **Sources Consulted**: `src/main.rs`(`ratatui::init`/`EnableMouseCapture` ~ `run_loop` ~ `DisableMouseCapture`/`ratatui::restore`), `src/app/mod.rs`(`Control` 정의).
- **Findings**: `run_loop`가 매 반복 `crossterm::event::poll`로 이벤트를 기다리다 `update()`를 호출하는 구조. `mouse_capture_enabled`는 `main()`의 로컬 변수라 `AppState`엔 없다(캡처 실패 시 조용히 무시하는 기존 결정과 일치).
- **Implications**: 편집 요청은 리듀서가 데이터(경로)만 반환하고, 실제 raw mode/대체화면/마우스 캡처 토글과 자식 프로세스 실행은 이미 그 상태를 쥔 `run_loop`가 담당해야 계층을 안 깨뜨린다.

### `$EDITOR`/`$VISUAL` 관례
- **Context**: 폴백 우선순위와 값 파싱 방식 확인.
- **Sources Consulted**: POSIX 관례(`vi`가 최종 폴백), git/crontab 등 기존 도구의 `$EDITOR` 처리 관행.
- **Findings**: 값에 인자가 포함될 수 있어(`"code -w"`, `"vim -u NONE"`) 공백 기준 첫 토큰을 실행 파일, 나머지를 고정 인자로 다루고 대상 파일 경로를 마지막에 추가하는 게 표준 관행이다. 셸(`sh -c`)을 거치면 경로·에디터 문자열에 셸 메타문자가 섞였을 때 인젝션 여지가 생기므로 피한다.
- **Implications**: `resolve_editor()`는 `Vec<String>`(프로그램 + 고정 인자)을 반환하고, 실행은 `Command::new(prog).args(rest).arg(path)`로 셸을 거치지 않는다.

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| 리듀서-신호 / 메인-실행 분리 (선택) | 리듀서는 `Control::EditFile(path)`만 반환, 실행은 `main` | 기존 계층 분리(`app`는 터미널을 모름) 유지, 테스트 가능한 순수 함수 경계 명확 | `Control`에 새 변형 → `main.rs`의 모든 매치 지점에 컴파일 강제 파급 | 채택 |
| 리듀서 안에서 직접 `Command::spawn` | `update()`가 프로세스를 직접 실행 | 변경 지점 하나로 줄어듦 | `app`가 터미널/프로세스를 알게 돼 `lib.rs` 의존 방향(`app`→`ui`→`main`) 위반, 리듀서 테스트가 실제 프로세스를 띄우게 됨 | 기각 |
| 외부 `edit` 크레이트 채택 | crates.io의 `edit` 등 "문자열을 $EDITOR로 편집" 크레이트 사용 | 구현 코드 감소 | 우리 자신의 이미 열려 있는 ratatui/crossterm 터미널 상태(raw mode·대체화면·마우스 캡처)를 모르므로 그 조율은 결국 직접 짜야 함 — 크레이트가 대신해 주는 부분이 거의 없음 | 기각(Build vs Adopt) |

## Design Decisions

### Decision: `Control::EditFile(PathBuf)` — 리듀서는 신호만, 실행은 main
- **Context**: 터미널 소유권이 `main`에만 있다는 기존 구조를 깨지 않고 편집 요청을 전달해야 함.
- **Alternatives Considered**: 위 표의 세 옵션.
- **Selected Approach**: `app::Control`에 `EditFile(PathBuf)` 변형 추가. `Action::Edit`는 문서 패널에 표시된 파일이 있으면 이 변형을, 없으면 `Continue`(+ 안내 팝업)를 반환.
- **Rationale**: `Control::Continue|Quit`와 대칭적인 세 번째 "다음 할 일" 신호일 뿐이라 기존 패턴과 자연스럽게 어울리고, 리듀서는 여전히 프로세스·터미널을 모르는 순수 함수로 남는다.
- **Trade-offs**: `main.rs`의 `Control` 매치가 하나 늘어남(컴파일 타임에 강제되므로 누락 위험은 낮음).
- **Follow-up**: 없음.

### Decision: 에디터 실행은 셸을 거치지 않고 프로그램+인자 배열로
- **Context**: `$EDITOR="code -w"`처럼 값에 인자가 섞여 올 수 있고, 셸 경유는 인젝션 표면을 늘림.
- **Selected Approach**: `resolve_editor(cli_editor: Option<&str>) -> Vec<String>`가 우선순위(`--editor` → `$VISUAL` → `$EDITOR` → `"vi"`)로 고른 문자열을 공백 기준으로 토큰화해 반환. 호출부는 `Command::new(&tokens[0]).args(&tokens[1..]).arg(path)`로 실행.
- **Rationale**: git 등 기존 도구와 동일한 관행, 셸 미경유로 인젝션 표면 최소화.
- **Trade-offs**: 따옴표로 묶인 인자가 있는 복잡한 `$EDITOR` 값(드묾)은 지원 범위 밖 — 필요해지면 후속 스펙에서 `shell-words` 같은 파서 채택 검토.

### Decision: 재렌더는 새 로직 대신 기존 `Action::Fs` 경로 재사용
- **Context**: 에디터 종료 후 파일이 바뀌었을 수 있는데, 이미 파일워처가 "경로 하나 변경 → 그 문서만 재렌더" 분류를 갖고 있음.
- **Selected Approach**: 편집 감시가 켜져 있으면(`state.watch == Live`) `main`이 자식 종료 직후 `Action::Fs(FsEvent{paths: vec![path]})`를 합성해 넣는다. 꺼져 있으면(`Manual`, `--no-watch`) 아무것도 하지 않는다(요구 3.2).
- **Rationale**: 새 "이 파일 강제 재로드" 로직을 또 만들지 않고 기존 경로를 그대로 태워, 스크롤 위치 보존 등 기존 재렌더 규칙을 자동으로 물려받는다(Build vs Adopt — 이미 있는 것을 재사용).
- **Trade-offs**: 없음.

## Risks & Mitigations
- 자식 프로세스가 터미널을 비정상적으로 남기고 죽어도(SIGKILL 등), 복귀 시퀀스가 raw mode·대체화면·마우스 캡처를 조건 없이 다시 강제하므로 화면은 정상 복원된다 — mitigated by design.
- `--editor`/`$EDITOR`/`$VISUAL` 값이 실행 불가능한 명령이면 `Command::spawn`이 `Err`를 반환 — `Action::EditFailed`로 안내, 뷰어는 계속 동작.
- 실제 raw mode 토글은 진짜 tty가 아닌 환경(CI 등)에서 실패할 수 있음 — Acceptance(L1) 검증은 실제 터미널에서 수동 확인으로 넘긴다(기존 main.rs의 "실행→종료→복원" 검증과 동일한 한계).

## References
- 없음(신규 외부 라이브러리 도입 없음, `design-discovery-light.md` §3 "For New Libraries Only" 비적용).
