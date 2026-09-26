# Brief: spec-viewer-tree-navigation-modes

## Problem
spec-viewer 사용자가 실행 중에 (1) "전체 모드"(`--all`, `.kiro`/spec-kit 무시하고 순수 마크다운 트리)와 "스펙 모드"(`.kiro`/`.specify` 자동 인식) 사이를 전환할 수 없고(프로세스를 다시 실행해야만 가능), (2) 트리 전체를 한 번에 펼치거나 접는 명시적 기능이 없어 깊은 트리를 탐색할 때 한 단계씩만 여닫아야 한다.

## Current State
- `--all`은 `main.rs`의 `resolve_source`가 시작 시점에 딱 한 번 평가하는 CLI 플래그다. `AppState.root: TreeSource`는 생성 이후 같은 variant 내부에서만 갱신되고(`resync`가 `Kiro`/`SpecKit`/`Files` 각각을 "같은 종류로" 다시 스캔), 다른 variant로 교체되는 코드는 어디에도 없다.
- `AppState.kiro_root: PathBuf`는 감시 대상이자 재스캔 기준 경로다 — `.kiro` 모드에선 `.kiro/` 자체, spec-kit 모드에선 `specs/`, Files 모드에선 스캔 대상 디렉터리. `resolve_source`가 원래 인자로 받은 시작 경로(`args.path` 또는 cwd)는 `Startup`/`AppState` 어디에도 보존되지 않는다 — 즉 "원래 어디서 출발했는지"를 지금은 잊어버린다.
- Left/Right/Enter/클릭에 의한 노드 펼침·접힘은 이미 `tui_tree_widget::TreeState`의 `key_left`/`key_right`/`toggle` API로 1단계(직계 자식)만 여닫는다(재귀 없음) — 이 동작 자체는 바뀌면 안 된다. "전체 펼치기/접기" 기능은 앱 코드에도 라이브러리에도 없다(`TreeState::close_all()`은 라이브러리에 있지만 미사용, `open_all` 상당 기능은 라이브러리에도 없음).
- `TreeMode`(`Auto`/`Always`/`Hidden`/`Single`, `1`~`4` 키)는 트리 "펼침 정도"가 아니라 "패널이 화면에서 차지하는 비율/표시 여부"에 관한 것이라 이번 기능과 개념적으로 무관하다(이름의 "모드"만 우연히 겹침).
- `Control::EditFile(PathBuf)` 선례: 리듀서가 스스로 못 하는 일(터미널/프로세스 제어)을 `main`에 신호로 넘기는 기존 패턴이 이미 있다. 파일 감시 스레드(`watch::Watch`)의 시작/교체도 지금은 `main()`만 할 수 있다 — 모드 전환이 감시 대상 디렉터리 자체를 바꾸므로, 감시 스레드를 다시 시작해야 할 가능성이 높다.

## Desired Outcome
- 사용자가 키 하나로 "전체 모드"↔"스펙 모드"를 실행 중에 전환할 수 있다. `--all`은 시작할 때 어느 모드로 켤지 정하는 초기값 역할만 하게 된다.
- 사용자가 키 하나로 트리 전체를 펼치거나 접을 수 있다(명시적 액션). 기존의 개별 노드 펼침·접힘(Left/Right/Enter/클릭)은 여전히 1단계씩만 동작하고 절대 재귀적으로 바뀌지 않는다.

## Approach
[design 단계에서 더 다듬을 후보]
- 모드 전환: 리듀서가 새 `Control` 변형(예: `Control::SwitchRoot(...)` 상당)을 반환해 `main`이 필요시 파일 감시를 새 디렉터리로 재시작하게 하는 방향이 유력(`Control::EditFile` 선례와 동일한 패턴). 단, "원래 시작 경로"를 어디에 보존할지(`AppState`에 필드 추가 vs `main`이 계속 들고 있다가 신호 처리 시 넘겨줌)는 design 단계에서 결정.
- 전체펼치기/접기: 순수 `TreeState` 조작이라 필터/워커 없이 `app::update` 안에서 완결 가능해 보임 — 새 `Action`(예: `ExpandAll`/`CollapseAll`) 두 개 추가로 충분할 가능성이 높음. `close_all()`을 재사용하고, "펼치기"는 현재 트리 구조를 순회해 모든 노드 경로를 `opened`에 채우는 헬퍼가 필요.

## Scope
- **In**: 실행 중 전체모드/스펙모드 전환 키, 전체펼치기/전체접기 키, `--all`의 의미를 "초기값"으로 재정의, 기존 1단계 펼침/접힘 동작 무회귀.
- **Out**: `TreeMode`(패널 레이아웃) 변경, 정렬(`SortKey`) 변경, 스펙모드 내에서 `.kiro`↔spec-kit 전환(이건 파일시스템이 결정하는 것이지 사용자가 고르는 게 아님 — 대상 밖), 트리 검색·선택 상태의 모드 간 승계(모드가 바뀌면 트리 구조 자체가 바뀌므로 선택 초기화가 기본값일 가능성이 높음, design 단계에서 확정).

## Boundary Candidates
- `src/app/mod.rs`: 신규 `Action`(모드 전환, 전체펼치기, 전체접기), 신규 `Control` 변형(모드 전환 시 감시 재시작 신호), `AppState`에 "원래 시작 경로"나 "현재 모드" 관련 필드 추가 가능성.
- `src/app/keymap.rs`: 신규 키 바인딩 3개.
- `src/main.rs`: `Control`의 새 변형 처리(감시 스레드 재시작), `--all`의 "초기값" 의미 재확인.

## Out of Boundary
- 파일 감시 라이브러리(`notify`) 자체 교체, 트리 위젯(`tui-tree-widget`) 교체.

## Upstream / Downstream
- **Upstream**: 없음.
- **Downstream**: `spec-viewer-editor-mode`(문서 패널 상태와 무관해 보이지만, 모드 전환 시 현재 열려 있던 문서가 사라지는 경우 편집모드와의 상호작용을 확인해야 함), `spec-viewer-spec-kit-support`(스펙모드 자동 인식 로직을 그대로 재사용).

## Existing Spec Touchpoints
- **Extends**: `spec-viewer`(원 스펙의 트리 탐색/`--all` 요구사항과 얽힘).
- **Adjacent**: `spec-viewer-editor-mode`, `spec-viewer-spec-kit-support`(둘 다 무관하게 병행 가능, 단 모드 전환 설계 시 두 기능과의 상호작용은 확인 필요).

## Constraints
- 기존 1단계 펼침/접힘 동작(Left/Right/Enter/클릭)은 절대 재귀적으로 바뀌면 안 된다.
- "전체펼치기/전체접기"는 반드시 별도의 명시적 액션이어야 한다(암묵적/자동 트리거 금지).
- `--all`은 여전히 CLI로 초기 모드를 지정하는 용도로 남는다(제거하지 않음).
- 파일 읽기 전용 원칙 유지.
