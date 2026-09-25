# Brief: spec-viewer-spec-kit-support

## Problem
GitHub spec-kit(`specify` CLI, `.specify/` + `specs/<NNN-이름>/{spec.md,plan.md,tasks.md,research.md,data-model.md,quickstart.md,contracts/}`)로 관리되는 프로젝트를 spec-viewer로 열면, 지금은 `--all` 모드(스키마 무관 순수 마크다운 트리)로만 볼 수 있어 기능별 진행 상태·승인/완료 여부를 트리에서 한눈에 파악할 수 없다. `.kiro` 프로젝트에서 누리는 것과 동등한 "상태를 아는" 브라우징 경험을 spec-kit 프로젝트에도 제공하고 싶다.

## Current State
- `src/spec/meta.rs`가 `.kiro/specs/<name>/spec.json`의 `phase` 문자열과 `approvals`(`requirements`/`bugfix`/`bizProcess`/`design`/`tasks` 키, 각 `{generated, approved}`) 맵을 파싱해 `SpecMeta`로 정규화하고, `tree_panel::spec_item`/`doc_item`이 이를 승인 게이트 진행률(`n/total`, tasks.md 방식과 동일 배지)과 문서별 상태 심볼(`●/○/·/?`)로 렌더링한다(직전 세션에서 phase 문자열 노출 → 게이트 카운트 배지로 바꿨다가, "completed"처럼 4개 게이트로는 설명 안 되는 후속 상태나 `bugfix.md` 단독 경로 등 실제 상태 조합이 더 다양하다는 게 드러나 원상복귀함 — 이 스펙의 설계 단계에서 이 교훈을 반드시 반영해야 함).
- `TreeSource` 열거형은 `Kiro(SpecRoot)`/`Files(FsTree)` 둘뿐이다. `Files`는 배지 개념이 아예 없는 순수 폴더 트리(`--all` 모드, requirement 1.9)라 spec-kit의 "산출물 진행 상태" 요구를 채우지 못한다.
- `main.rs`의 `find_root`(`src/spec/mod.rs`)는 `.kiro` 디렉터리만 위로 탐색해 루트로 인식한다. spec-kit은 루트 마커가 `.specify/`이고, 실제 기능별 산출물은 그 옆의 `specs/<NNN-이름>/`(예: `specs/003-chat-system/spec.md`)에 있다 — `.specify/` **내부**가 아니라 프로젝트 루트의 형제 디렉터리라는 점이 `.kiro`와 다르다.
- spec-kit에는 `.kiro`의 `phase` 같은 단일 상태 필드가 없다. `.specify/feature.json`은 "지금 작업 중인 기능 하나"를 가리키는 포인터일 뿐, 전체 기능 목록의 상태 인덱스가 아니다. 각 기능의 진행 정도는 `spec.md`/`plan.md`/`tasks.md` 등 산출물 파일의 **존재 여부**로 유추해야 한다(예: `spec.md`만 있으면 "명세 완료", `+plan.md`면 "설계 완료", `+tasks.md`면 "작업 분해 완료" — 그 이후 실제 구현 완료 여부는 spec-kit 자체도 파일 하나로 명시적으로 표시하지 않는다, `/speckit-converge`가 "Converged"를 보고할 뿐 파일에 기록하진 않음).
- 직전 세션에서 트리 배지를 "n/total 승인 게이트 개수"로 단순화했다가 사용자가 반려한 사건이 있었다: `phase: completed`(4/4 게이트 승인 이후, 실제 구현+검증까지 끝난 후속 상태)와 "4/4 게이트만 승인되고 아직 구현 중"인 상태가 똑같이 "4/4"로 보여 구분이 안 됐고, 개선안(완료 시 별도 마커 추가)도 `bugfix.md` 경로처럼 게이트 4개 모델 자체에 안 맞는 경우를 여전히 놓쳤다. **결론: 상태를 고정된 "게이트 개수" 하나로 뭉개면 안 되고, 가변 길이의 "마일스톤 목록 + 각 마일스톤의 상태" 같은 더 느슨한 모양이어야 `.kiro`의 비균일성(bugfix 전용 경로 포함)과 spec-kit의 파일-존재-기반 모델을 둘 다 왜곡 없이 담을 수 있다.**

## Desired Outcome
- `.specify/` 마커가 있는 디렉터리(또는 그 하위)에서 `m`을 실행하면 `.kiro`처럼 자동으로 루트를 찾아 `specs/<NNN-이름>/` 트리를 조회 전용으로 브라우징한다.
- 트리에서 각 기능 노드가 산출물 존재 여부에 기반한 진행 상태를 보여주고, `tasks.md`가 있으면 기존 체크박스 진행률(`n/m`) 로직을 그대로 재사용해 보여준다.
- `tree_panel`의 렌더링 코드가 `.kiro`/spec-kit을 소스별로 분기하지 않고, 두 스키마 모두를 왜곡 없이 표현할 수 있는 공통 상태 모양에 대해서만 동작한다(SSoT) — 단, 이 공통화가 `.kiro`의 기존 표시(요구사항 3.1~3.5)를 퇴행시키지 않아야 한다(이번에 반려된 시도가 왜 부족했는지가 검증 기준이 된다).

## Approach
설계 단계(`kiro-spec-design`)에서 `design-synthesis.md` 룰에 따라 더 다듬을 후보들 — 이 브리프에서는 확정하지 않는다:

- **A. 완전 분리**: `TreeSource::SpecKit(...)` 신규 variant + 전용 파싱/렌더 경로. 빠르지만 공통화 요구를 충족 못 함(반려 후보, 사용자가 명시적으로 공통화를 요구함).
- **B. 정규화 레이어**: 두 소스 모두 파싱 단계에서 "가변 길이 마일스톤 리스트"(예: `Vec<Milestone { name, state: Done|Generated|Missing|NotApplicable }>`) 같은 중립 모델로 정규화하고, `tree_panel`은 그 중립 모델만 보고 렌더링. `.kiro`의 마일스톤 = approvals 키들(있는 것만, bugfix 경로는 자연히 더 짧은 리스트), spec-kit의 마일스�트 = spec.md/plan.md/tasks.md 등 산출물 존재. "완료" 같은 4게이트로 설명 안 되는 후속 상태는 마일스톤 리스트에 명시적 마일스톤 하나를 더 추가하는 식으로 자연스럽게 수용 가능한지가 설계 단계의 핵심 검증 지점.
- **C. 소스 트레이트**: `SpecSource` 트레이트로 완전히 열어, 향후 다른 방법론(예: 또 다른 SDD 툴)도 같은 인터페이스로 추가 가능하게. 확장성은 가장 크지만 설계/구현 비용도 가장 큼 — 이 스펙 하나만 놓고 보면 과설계일 수 있음(YAGNI 검토 필요).

## Scope
- **In**: `.specify/` 루트 자동 인식, `specs/<NNN-이름>/` 트리 조회(spec.md/plan.md/tasks.md/research.md/data-model.md/quickstart.md/contracts/ 표시), 산출물 존재 기반 진행 상태 배지, tasks.md 체크박스 진행률 재사용, `.kiro`/spec-kit 공통 상태 모델 설계 및 이번에 반려된 케이스(completed 후속 상태, bugfix 전용 경로) 회귀 검증.
- **Out**: `specify` CLI 실행/연동(init, plan, tasks 등 생성 커맨드), `.specify/extensions`·`presets`·`hooks`·`checklists` 같은 고급 기능 지원, 파일 편집(읽기 전용 유지 — `spec-viewer-editor-mode`의 `e` 키로 여는 것 자체는 이 스펙과 무관하게 이미 지원됨), git 확장(numbered feature branch) 연동, `contracts/` 디렉터리 내부 파일 형식(OpenAPI 등)별 특수 렌더링.

## Boundary Candidates
- `src/spec/`(신규 spec-kit 파서 모듈 + 기존 `meta.rs`/`progress.rs`와의 공통 추상화 리팩터링)
- `src/ui/tree_panel.rs`(소스별 분기 없는 공통 배지 렌더링으로 일반화)
- `src/main.rs`(`find_root`/`resolve_source`에 `.specify/` 자동 인식 추가)

## Out of Boundary
- spec-kit 프로젝트 생성/수정, `.specify/` 설정 자체 편집, spec-kit의 `/speckit-*` 커맨드 실행/시뮬레이션.

## Upstream / Downstream
- **Upstream**: 없음 — `github/spec-kit` 저장소 구조는 조사 참고 대상일 뿐 코드/패키지 의존성은 없음(신규 외부 크레이트 도입도 가급적 피함).
- **Downstream**: 기존 `.kiro` 지원(원 `spec-viewer` 스펙의 요구사항 3.1~3.7)과 `--all` 모드(요구사항 1.9) 둘 다 공통화 리팩터링의 영향을 받는 회귀 위험 구간.

## Existing Spec Touchpoints
- **Extends**: `spec-viewer`(원 스펙의 트리 상태 표시 요구사항 3.1~3.7과 직접 얽힘 — 이번 리팩터링으로 해당 요구사항 문구도 갱신 필요할 수 있음)
- **Adjacent**: `spec-viewer-editor-mode`(무관, 병렬 진행 가능 — `e` 키로 spec-kit 문서를 여는 것도 자동으로 동작해야 하며 별도 작업 불필요할 것으로 예상)

## Constraints
- 파일 읽기 전용 원칙 유지 — 어떤 경로로도 `specs/`·`.specify/` 아래를 쓰지 않는다.
- 신규 외부 의존성 최소화 — 가능하면 기존 `pulldown-cmark`/`serde_json`만으로 spec.md/plan.md/tasks.md 파싱(YAML frontmatter가 필요하면 그때 검토).
- 기존 `.kiro` 사용자에게 회귀 없어야 함 — 전체 회귀 테스트 통과 필수, 특히 이번에 반려된 "completed 상태 소실" 케이스와 "bugfix 전용 경로" 케이스를 설계 단계에서 명시적으로 커버해야 한다.
- 사용자 지시: 공통화를 "적극적으로 검토"하되, `.kiro` 자체의 비균일성(승인 게이트 개수가 스펙마다 다름, bugfix 경로 포함)도 함께 수용 가능한 유연한 모양으로 설계할 것 — 고정 폭(예: 항상 4개) 모델로 되돌아가지 말 것.
