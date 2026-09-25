# Research & Design Decisions — spec-viewer-spec-kit-support

## Summary
- **Feature**: `spec-viewer-spec-kit-support`
- **Discovery Scope**: Full discovery(에스컬레이션) — `design-discovery-light.md`의 에스컬레이션 조건("significant architectural changes needed") 충족: 사용자가 `.kiro`/spec-kit 상태 표시를 공유 가능한 공통 추상화로 일반화할 것을 명시적으로 요구했고, 이는 기존 `Spec`/`TreeSource`의 형태 자체를 바꾸는 변경이다.
- **Key Findings**:
  - GitHub spec-kit의 실제 산출물 위치는 `.specify/`(설정/메모리/스크립트) **내부가 아니라**, 프로젝트 루트의 형제 디렉터리 `specs/<NNN-이름>/{spec.md,plan.md,tasks.md,research.md,data-model.md,quickstart.md,contracts/}`이다(`.specify/feature.json`은 "지금 작업 중인 기능 하나"만 가리키는 포인터이지 전체 목록 인덱스가 아님).
  - spec-kit엔 `.kiro`의 `phase`/`approvals` 같은 단일 상태 필드가 없다 — 진행 정도는 산출물 파일의 **존재 여부**로만 유추 가능하고, "구현 완료·검증됨"을 알려주는 파일도 없다(`/speckit-converge`는 "Converged"를 보고만 할 뿐 기록하지 않음) — 이는 spec-kit 자체의 한계로 그대로 인정한다(요구사항 4.5).
  - `.kiro` 자체도 이미 균일하지 않다: `approvals` 맵은 `spec.json` 작성자가 그때그때 키를 추가하는 방식이라 스펙마다 길이가 다르고(`bugfix.md` 전용 경로는 1~2개뿐), "게이트 전부 승인"과 "phase: completed"(구현+검증까지 끝난 후속 상태)는 서로 다른 시점인데 지난 세션에서 트리 배지를 "n/4 게이트 개수"로 단순화했다가 이 둘을 구분 못 해 반려됐다.
  - 기존 코드(`src/spec/mod.rs`)의 `DocEntry{kind: DocKind, path, exists, status: DocStatus, progress}`는 이미 소스 무관하게 재사용 가능한 모양이다 — `DocKind::Other(String)` + `DocStatus::NotTracked`(배지 없음, 존재 여부만 반영)가 정확히 spec-kit 문서(승인 개념 없음, 존재만 있음)에 맞는다. `progress: Option<Progress>`/`count_progress`도 순수 텍스트 체크박스 카운트라 이미 소스 무관.
  - 소스 무관하지 않은 것은 딱 하나, "스펙/기능 전체의 진행 배지"뿐이다 — `.kiro`는 `spec.json`의 `approvals` 맵에서, spec-kit은 `spec.md`/`plan.md`/`tasks.md` 존재에서 나온다.

## Research Log

### spec-kit 디렉터리 구조
- **Context**: `.specify/`가 `.kiro/`와 같은 위치 관계(마커 디렉터리 = 산출물 루트)인지 확인 필요.
- **Sources Consulted**: `github/spec-kit` 저장소의 `docs/quickstart.md`, `docs/reference/core.md`, `spec-driven.md`(`gh api repos/github/spec-kit/contents/...`로 직접 조회).
- **Findings**: `specs/[branch-name]/`(예: `specs/003-chat-system/spec.md`)가 프로젝트 루트에 생기고, `.specify/`는 설정·스크립트·메모리 전용이다. `SPECIFY_INIT_DIR`/`SPECIFY_FEATURE_DIRECTORY` 환경변수로 프로젝트/기능을 각각 독립적으로 재지정할 수 있다는 것도 확인했으나(2축 해석), 이 스펙은 CLI 연동이 범위 밖이라 참고만 함.
- **Implications**: `.kiro`의 `find_root`는 마커 디렉터리 자체를 반환하지만(그 안에 바로 `specs/`가 있음), spec-kit용 루트 탐색 함수는 **`.specify/`의 부모(프로젝트 루트)** 를 반환해야 `project_root.join("specs")`가 같은 방식으로 동작한다 — 반환값의 의미가 서로 다르다는 것을 설계에 명시해야 한다.

### `.kiro` 상태 배지의 기존 비균일성
- **Context**: 지난 세션에 트리 배지를 "n/4 승인 게이트"로 바꿨다가 사용자가 두 가지 이유로 반려함: (1) `phase: completed`(구현+검증 후속 상태)와 "게이트 전부 승인"이 똑같이 보임, (2) `bugfix.md` 전용 경로처럼 게이트가 4개가 아닌 경우를 개선안도 못 담음.
- **Sources Consulted**: 이 저장소의 `src/spec/meta.rs`(`DocKind`가 `Requirements/Bugfix/BizProcess/Design/Tasks`로 이미 가변 — `approvals` 맵에 실제로 기록된 키만 들어감, 픽스처 `bugfix-only`/`sample-signup` 확인), `tests/fixtures/kiro/specs/*/spec.json`.
- **Findings**: `approvals` 맵은 이미 가변 길이다(스펙마다 총 개수가 다름) — 문제는 개수 자체가 아니라 "게이트 전부 승인"과 "그 이후의 완료 상태"를 배지 하나(`n/total`)로 뭉뚱그린 것이었다.
- **Implications**: "마일스톤" 목록에 `phase == "completed"`일 때만 나타나는 **추가 마일스톤**을 하나 더 넣으면(예: 게이트 4개가 이미 다 있는 스펙이 `completed`가 되면 5번째 마일스톤이 생겨 `4/4`가 아니라 `5/5`로 바뀜), 같은 `n/total` 표시 방식을 유지하면서도 두 시점이 시각적으로 구분된다. `bugfix.md` 전용 경로도 `approvals` 맵에 실제로 있는 키만 마일스톤이 되므로 자연히 정확하게 추적된다(이미 그랬던 동작 그대로, 완료 마일스톤만 조건부 추가).

## Architecture Pattern Evaluation

| Option | Description | Strengths | Risks / Limitations | Notes |
|--------|-------------|-----------|---------------------|-------|
| A. 완전 분리 | `TreeSource::SpecKit`이 전용 파싱·전용 렌더 함수를 처음부터 끝까지 따로 가짐 | 구현 빠름, 기존 `.kiro` 코드 무변경 | 사용자가 명시적으로 요구한 "렌더링 공유"를 충족 못 함, 배지 형식이 두 곳에서 각자 진화해 다시 벌어질 위험 | 반려 |
| B. 공통 배지 뷰 + 각자 도메인 모델 (채택) | `Spec`에 소스 무관 `milestones: Vec<Milestone>`/`warning: Option<String>` 필드를 추가하고, `tree_panel`의 배지 렌더링 함수 하나를 두 소스가 공유. `DocEntry`/`DocKind::Other`/`Progress`는 이미 무관하므로 그대로 재사용. `.kiro` 전용 `kiro_meta`(구 `meta`)는 정렬 전용으로 남김 | "생성 인터페이스만 일반화, 구현은 각자" 원칙에 부합. 새 트레이트 없이 기존 `TreeSource` enum 패턴 그대로 확장(이 코드베이스가 이미 쓰는 패턴). 배지 스타일 중복이 구조적으로 불가능해짐 | `Spec` 필드 리네임(`meta`→`kiro_meta`)이 기존 `.kiro` 전용 테스트를 다수 건드림(roadmap.md에 이미 위험으로 명시) | 채택 |
| C. `SpecSource` 트레이트로 완전 추상화 | Kiro/SpecKit(향후 다른 방법론까지)을 트레이트 구현체로 | 확장성 최대 | 이 스펙엔 구현체가 2개뿐이라 트레이트 오브젝트/제네릭 경계를 새로 도입하는 비용 대비 이득이 없음(YAGNI), 이 코드베이스에 트레이트 기반 소스 추상화 선례가 없어 스타일 이탈 | 기각(과설계, `design-synthesis.md` "Simplification": 두 번째 구현체까지만 있으면 트레이트 없이도 충분) |

## Design Decisions

### Decision: 소스 무관 "마일스톤" 목록으로 진행 배지 일반화
- **Context**: `.kiro`의 승인 게이트 개수와 spec-kit의 산출물 존재 개수를 같은 `n/total` 배지로 보여줘야 하고, `.kiro`의 "게이트 전부 승인 vs 구현 완료" 구분도 되살려야 한다.
- **Alternatives Considered**: 위 표의 A/C.
- **Selected Approach**: `Spec`에 `pub milestones: Vec<Milestone>`(`Milestone{name: String, done: bool}`) 추가. `.kiro` 빌더는 `approvals` 맵의 각 키를 마일스톤 하나로 변환하고, `phase == "completed"`일 때만 "구현 완료" 마일스톤을 추가로 붙인다. spec-kit 빌더는 `spec.md`/`plan.md`/`tasks.md` 존재를 각각 고정된 3개 마일스톤으로 변환한다(둘 다 `done`이 몇 개인지는 각자 계산, `tree_panel`은 개수만 본다).
- **Rationale**: "인터페이스만 일반화, 구현은 각자"에 정확히 부합 — 두 소스의 "총 개수를 정하는 규칙"은 서로 다르게 둬도(`.kiro`는 기록된 키만큼 가변, spec-kit은 항상 3) `Vec<Milestone>`이라는 모양 자체는 같아서 렌더링 코드가 갈라지지 않는다.
- **Trade-offs**: `.kiro` 스펙이 `spec.json`을 아직 갖지 않은 초기 상태(마일스톤 0개)에서는 배지가 아예 안 뜬다 — 기존 동작(빈 `approvals` 맵 → 배지 없음)과 동일하므로 회귀 아님.
- **Follow-up**: "구현 완료"를 트리거하는 정확한 문자열(`"completed"`)은 이 저장소 자체의 관행일 뿐 `.kiro` 표준이 아니다 — design.md의 Revalidation Triggers에 명시.

### Decision: 경고 배지도 소스 무관 필드로 분리(`warning: Option<String>`)
- **Context**: 기존 경고 배지(`spec.json` 파싱 실패)는 `spec.meta.is_err()`로 트리거됐는데, spec-kit엔 그런 단일 파일이 없어 이 트리거를 그대로 쓸 수 없다.
- **Selected Approach**: `Spec.warning: Option<String>`을 새로 두고 `.kiro` 빌더만 채운다(`meta`가 `Err`일 때 그 메시지를). spec-kit 빌더는 항상 `None`(요구사항에 spec-kit 파싱 실패 경고가 없음).
- **Rationale**: 배지 렌더링 함수가 "경고가 있는가"를 소스 판별 없이 한 필드만 보고 결정할 수 있다.
- **Trade-offs**: 없음 — 순수 추가/분리.

### Decision: `find_root`와 짝을 이루는 `find_spec_kit_root`는 "프로젝트 루트"를 반환(`.specify/` 자체가 아님)
- **Context**: `.kiro`의 `find_root`는 마커 디렉터리 자체(`.kiro/`)를 반환하고 그 안에 바로 `specs/`가 있다. spec-kit은 `specs/`가 `.specify/`의 형제라 마커 자체를 반환하면 `root.join("specs")`가 틀린 경로가 된다.
- **Selected Approach**: `find_spec_kit_root(start) -> Option<PathBuf>`는 `.specify/`를 찾되 **그 부모 디렉터리**를 반환해, 호출부가 `.kiro`든 spec-kit이든 항상 `root.join("specs")`로 같은 방식으로 접근하게 한다.
- **Trade-offs**: 두 함수의 반환값 의미가 미묘하게 다르다는 것을 design.md에 명시하지 않으면 다음 리팩터링 때 혼동 위험 — Revalidation Triggers에 기록.

### Decision: `TreeSource::SpecKit`은 `SpecRoot`를 재사용하지 않고 `Vec<Spec>` 그대로
- **Context**: `SpecRoot{specs, steering}`을 그대로 재사용하면 spec-kit엔 없는 `steering`이 항상 빈 채로 존재하게 된다.
- **Selected Approach**: `TreeSource::SpecKit(Vec<Spec>)`로 두고, 렌더링에서 Steering 그룹 노드 자체를 만들지 않는다.
- **Rationale**: `design-synthesis.md`의 Simplification — 항상 비어 있을 필드를 타입에 억지로 넣지 않는다.

## Risks & Mitigations
- `Spec.meta` 필드 리네임(`kiro_meta`)과 `milestones`/`warning` 추가가 기존 `.kiro` 전용 테스트를 대량으로 건드림 — 전체 회귀 스위트를 각 태스크마다 자주 돌려 조기에 잡는다(roadmap.md에 이미 명시).
- spec-kit 실제 프로젝트로 수동 검증할 수 없음(이 저장소 자체엔 spec-kit 프로젝트 픽스처가 없음) — `tests/fixtures/spec-kit/`류 픽스처를 새로 만들어 태스크 단계에서 커버한다.
- `phase == "completed"` 하드코딩 문자열이 이 저장소만의 관행 — 다른 `.kiro` 사용자가 다른 문자열을 쓰면 "구현 완료" 마일스톤이 안 붙는다(기능 저하일 뿐 오류는 아님) — Revalidation Triggers에 명시.

## References
- [Spec Kit Quickstart](https://github.com/github/spec-kit/blob/main/docs/quickstart.md)
- [Spec Kit Core Commands Reference](https://github.com/github/spec-kit/blob/main/docs/reference/core.md)
- [spec-driven.md](https://github.com/github/spec-kit/blob/main/spec-driven.md)
