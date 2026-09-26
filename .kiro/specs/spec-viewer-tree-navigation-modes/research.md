# Research & Design Decisions — spec-viewer-tree-navigation-modes

## Summary
- **Feature**: `spec-viewer-tree-navigation-modes`
- **Discovery Scope**: Extension(기존 spec-viewer 확장) — `design-discovery-light.md` 적용. 에스컬레이션 조건(대규모 아키텍처 변경/외부 서비스 연동/보안 민감/성능 critical/미검증 의존성) 해당 없음 — `Control::EditFile` 선례를 그대로 재사용하는 확장.
- **Key Findings**:
  - `resolve_source`가 "시작 경로"(`start`)를 계산하는 로직이 `--all`/`.kiro`/`.md` 파일뷰 판정 세 곳에 각각 인라인돼 있고, 그 값 자체는 `Startup`/`AppState` 어디에도 보존되지 않는다.
  - `Control::EditFile(PathBuf)` → main이 부수효과 실행 → `Action::EditFailed`/`Action::Fs`로 결과 회신, 하는 3단 패턴이 이미 확립돼 있고 이번 기능(모드 전환)에 그대로 들어맞는다.
  - 파일 감시(`watch::Watch`)는 `main()`의 지역 변수로, `run_loop` 호출 전 만들어져 호출 후 명시적으로 drop된다 — 재시작하려면 기존 값을 교체하고 새 root로 `watch::start`를 다시 부르면 된다(`Sender<FsEvent>`는 `Clone`).
  - 검색용 `flatten_tree`가 이미 트리 전체를 `Vec<(Vec<NodeId>, String)>`로 평탄화해두므로, "전체펼치기"는 이 결과에서 폴더성 노드(`Spec`/`SteeringGroup`/`Dir`)만 골라 `TreeState::open()`을 반복 호출하면 새 순회 로직 없이 구현 가능. "전체접기"는 `TreeState::close_all()` 그대로.
  - 자유 키(소문자): `a c h i l m o p v w x y` — `m`(모드), `o`(전체펼치기), `c`(전체접기)가 전부 비어 있고 영어 mnemonic과도 맞음.

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| A. `AppState`에 "시작 경로" 필드 추가 | `start: PathBuf`을 상태에 저장해 리듀서가 직접 모드 판정 | 리듀서 자기완결적 | `app`가 디스크 I/O(상향 마커 탐색)를 직접 하게 돼 기존 의존 방향(app는 파일시스템 스캔 정도는 하지만 "탐색 경로 결정" 같은 부팅 로직까지는 안 가짐) 원칙을 흐림, `AppState::new` 시그니처 변경이 26곳(대부분 테스트)에 영향 | 기각 |
| B. `main`이 `start`를 지역 변수로 보유, `Control::SwitchMode` 신호로 위임 (채택) | `Control::EditFile` 선례와 동일한 패턴 — 리듀서는 "전환하고 싶다"는 신호만, 실제 판정/I/O/감시 재시작은 main | 기존 검증된 패턴 재사용, `AppState` 필드 추가 없음(26곳 영향 회피) | `resolve_source`의 우선순위 판정 로직을 재사용 가능한 함수로 뽑아내야 함(현재 인라인) | 채택 |
| C. 매번 프로세스 재시작 유도(모드 전환 키를 안내 메시지로만) | 구현 없음 | 최소 비용 | 요구사항(실행 중 전환)을 아예 충족 못 함 | 기각(요구사항 미충족) |

## Design Decisions

### Decision: `start` 경로는 `AppState`가 아니라 `main()`이 계속 들고 있는다
- **Context**: 모드 전환 시 "원래 어디서 출발했는지"를 다시 알아야 스펙모드 우선순위 판정을 재실행할 수 있다.
- **Selected Approach**: `main()`에서 이미 계산 중인 `start`(현재는 `resolve_source` 안에 있음)를 `main()` 스코프로 끌어올려 `run_loop`/새 핸들러에 전달한다.
- **Rationale**: `AppState::new`를 26개 호출부(대부분 테스트) 건드리지 않고, `app`가 부팅 시점 경로 탐색 책임을 지지 않게 한다(design-principles "Dependency direction" 준수).
- **Trade-offs**: "전체모드에서 보던 위치"는 스펙모드로 안 돌아가도 그대로 스펙모드 자동 인식 규칙이 다시 적용될 뿐 — 요구사항 1.6이 이미 "이전 상태 승계 안 함"을 명시했으므로 문제 없음.

### Decision: `resolve_source`의 우선순위 판정을 재사용 가능한 함수로 추출
- **Context**: 시작 시점(`resolve_source`)과 실행 중 전환(신규 핸들러) 둘 다 같은 "`.kiro`/`.specify` 중 뭘 고를지" 규칙이 필요하다.
- **Selected Approach**: `fn resolve_spec_mode(start: &Path) -> Result<(TreeSource, PathBuf), String>`로 뽑아 `resolve_source`와 신규 핸들러가 함께 호출한다.
- **Rationale**: SSoT — 우선순위 규칙이 두 곳에 따로 존재하면 나중에 한쪽만 고쳐지는 사고가 난다(직전 스펙들에서 반복적으로 겪은 문제 패턴).
- **Trade-offs**: `resolve_source`의 기존 `StartupError::RootNotFound` 기반 에러 처리와 신규 함수의 `String` 기반 에러 처리가 형태가 달라, `resolve_source`는 이 함수의 `Err`를 자신의 `StartupError`로 변환하는 얇은 래핑이 필요하다(design.md에 명시).

### Decision: 모드 전환 실패 메시지는 `Action::EditFailed`를 재사용하지 않고 별도 변형(`Action::SwitchModeFailed`)으로
- **Context**: "팝업에 메시지만 담는다"는 동작이 `Action::EditFailed`와 완전히 같아 보여 재사용 유혹이 있다.
- **Selected Approach**: 별도 `Action::SwitchModeFailed(String)`를 새로 둔다.
- **Rationale**: `Action::EditFailed`는 이미 승인된 다른 스펙(`spec-viewer-editor-mode`)의 산출물이고 그 이름 자체가 "편집 실패"라는 의미를 갖는다 — 의미가 다른 두 실패를 같은 이름으로 재사용하면 Descriptable Naming을 해치고, 다른 스펙의 승인된 코드를 이번 스펙이 건드리게 된다(경계 침범). 두 곳 다 `state.popup = Some(Popup::Message(...))` 한 줄뿐이라 중복 비용이 아주 작다.
- **Trade-offs**: 아주 작은 코드 중복(3~4줄) — Simplification보다 경계/네이밍 정확성을 우선.

### Decision: 전체펼치기는 새 순회 로직 없이 기존 `flatten_tree` 재사용
- **Context**: "모든 폴더성 노드를 편다"는 트리 전체를 훑어야 하는데, 이미 검색 기능이 트리 전체를 평탄화하고 있다.
- **Selected Approach**: `flatten_tree`가 반환하는 `Vec<(Vec<NodeId>, String)>`에서 각 항목의 마지막 `NodeId`가 `Spec`/`SteeringGroup`/`Dir`인 것만 걸러 그 경로에 `TreeState::open()`을 호출한다.
- **Rationale**: Simplification — 새 트리 순회 함수를 또 만들지 않는다(Build vs Adopt: 이미 있는 것을 재사용).
- **Trade-offs**: `flatten_tree`가 검색 목적으로 설계된 함수라 이 용도로 쓰기엔 다소 우회적이지만, 반환 형태가 정확히 필요한 것(전체 노드의 전체 경로)이라 실질적 비용은 없음.

## Risks & Mitigations
- `run_loop`/`main()`의 시그니처가 또 커진다(`args`, `tx`, `watch: &mut Watch` 추가 필요) — `spec-viewer-editor-mode`에서 이미 한 번 겪은 종류의 변경이라 패턴이 익숙함, 전체 회귀로 조기 검증.
- 모드 전환 중 파일 감시가 잠깐 끊기는 순간(옛 watch drop ~ 새 watch 시작 사이)이 있음 — 그 사이의 파일 변경은 놓칠 수 있으나, 전환 자체가 최신 상태를 다시 스캔해서 반영하므로(요구사항 1.5) 실질적 영향 없음.
- `AppState::new`를 안 건드리는 대신 `state.root`/`state.kiro_root`를 `main()`이 직접 mutate해야 함 — 두 필드가 이미 `pub`이라 가능하지만, "무엇이 원자적으로 같이 바뀌어야 하는지"(root+kiro_root+tree+doc+selection 전부 한 번에)를 놓치면 불일치 상태가 생길 수 있음 — Testing Strategy에서 명시적으로 검증.

## References
- 없음(신규 외부 라이브러리 도입 없음).
