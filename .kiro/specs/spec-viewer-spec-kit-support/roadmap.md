# Roadmap

## Overview
spec-viewer가 `.kiro` 방법론 프로젝트에 제공하는 "상태를 아는" 조회 전용 브라우징(트리 진행 배지, tasks.md 체크박스 진행률)을 GitHub spec-kit(`.specify/` + `specs/<NNN-이름>/`) 프로젝트에도 동등하게 제공한다. 사용자가 명시적으로 요구한 대로, `.kiro`와 spec-kit 두 소스가 `tree_panel`의 상태 렌더링 코드를 공유할 수 있는 공통 추상화를 적극적으로 검토하며 진행한다 — 단, 이번 세션에서 트리 배지를 성급히 "n/4 승인 게이트"로 단순화했다가 `completed` 후속 상태와 `bugfix.md` 전용 경로를 놓쳐 반려된 사건을 교훈 삼아, 고정 폭 모델이 아닌 가변 길이 모델을 전제로 설계한다.

## Approach Decision
- **Chosen**: 단일 스펙(Path 분해 없음). discovery/requirements까지는 하나의 스펙으로 진행하고, 공통 추상화의 구체적 형태(브리프의 A/B/C 후보)는 `kiro-spec-design`의 `design-synthesis.md`(generalization/build-vs-adopt) 룰로 그 단계에서 결정한다.
- **Why**: 이 기능은 "spec-kit 소스 추가"라는 단일 목적을 향한 하나의 응집된 변경이다. 파서(신규)·공통 상태 모델(리팩터링)·트리 렌더링(일반화)·루트 자동 인식(확장)이 서로 강하게 얽혀 있어 별도 스펙으로 쪼개면 오히려 "공통 모델이 무엇인지"를 한 스펙 안에서 끝까지 못 보고 인터페이스만 먼저 굳히는 위험이 있다 — `spec-viewer-editor-mode`(리듀서 신호/키맵/에디터 실행/run_loop 연결/E2E를 한 스펙으로 묶은 전례)와 같은 판단.
- **Rejected alternatives**:
  - *기존 `spec-viewer` 스펙에 태스크로 추가*: 이미 `phase: completed`로 닫힌 스펙이고, 이번 변경은 그 스펙의 요구사항 3.1~3.7 자체를 갱신할 만큼 크다 — 별도 스펙으로 두어 이력을 분리하는 편이 낫다(단, `Existing Spec Touchpoints`에 명시한 대로 원 스펙 문서는 갱신한다).
  - *"공통화" 부분만 먼저 별도 스펙으로 분리(리팩터링 스펙 + spec-kit 스펙 2개)*: 사용자가 공통화를 "적극적으로 검토"하라고 했지 확정하라고 하지 않았다 — 실제 spec-kit 요구사항을 모르는 채로 추상화부터 먼저 굳히면 YAGNI/과설계 위험이 크다. 공통화 여부·형태는 이 스펙의 design 단계에서 spec-kit 요구사항과 함께 결정한다.

## Scope
- **In**: brief.md의 Scope In 그대로 — `.specify/` 자동 인식, `specs/<NNN-이름>/` 조회, 진행 상태 배지, tasks.md 진행률 재사용, 공통 상태 모델 검토·적용.
- **Out**: brief.md의 Scope Out 그대로 — CLI 연동, 고급 spec-kit 기능, 편집, git 확장, contracts 특수 렌더링.

## Constraints
brief.md 그대로: 읽기 전용, 외부 의존성 최소화, `.kiro` 회귀 없음(특히 completed/bugfix 케이스), 가변 길이 공통 모델(고정 폭 회귀 금지).

## Boundary Strategy
- **Why this split**: 단일 스펙 안에서도 구현은 자연히 세 경계로 나뉜다 — (1) `src/spec/`의 파싱·정규화(신규 spec-kit 파서 + 기존 `.kiro` 파서와의 공통 모델), (2) `src/ui/tree_panel.rs`의 렌더링(공통 모델 소비, 소스 분기 제거), (3) `src/main.rs`의 루트 탐색(`.specify/` 인식 추가). 이 세 경계는 `spec-viewer-editor-mode`가 `app`/`keymap`/`main`으로 나눴던 것과 같은 결의 레이어 분리라 tasks.md 단계에서 그대로 재사용 가능하다.
- **Shared seams to watch**: `tree_panel::spec_item`/`doc_item`이 공통 모델에만 의존하게 바뀌면 `.kiro` 전용 테스트(예: `expanding_spec_reveals_docs_in_canonical_order_with_status_symbols`)가 대량으로 영향받는다 — 리팩터링 중 전체 회귀 스위트를 자주 돌려야 한다. `find_root`가 `.kiro`와 `.specify`를 동시에 찾을 때의 우선순위(둘 다 있는 디렉터리는 없다고 가정할 수 있는지, 있다면 어느 쪽 우선인지)도 design 단계에서 명시해야 한다.

## Specs (dependency order)
- [ ] spec-viewer-spec-kit-support -- `.specify/`+`specs/<NNN-이름>/` 조회 지원과 `.kiro`/spec-kit 공통 상태 모델. Dependencies: none
