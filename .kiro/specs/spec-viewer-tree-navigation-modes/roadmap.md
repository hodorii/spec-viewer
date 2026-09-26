# Roadmap

## Overview
실행 중 "전체 모드"↔"스펙 모드" 전환과 "전체펼치기/전체접기" 명시적 액션을 spec-viewer의 트리 패널에 추가한다. 기존 1단계 펼침/접힘 동작은 절대 바꾸지 않는다는 제약 아래, `Control::EditFile` 선례를 따라 리듀서가 할 수 없는 일(파일 감시 재시작)은 `main`에 신호로 넘기는 기존 아키텍처 패턴을 그대로 재사용한다.

## Approach Decision
- **Chosen**: 단일 스펙(Path 분해 없음). 두 기능(모드 전환, 전체펼치기/접기)을 하나의 스펙 안에서 각각 별도 태스크 그룹으로 다룬다.
- **Why**: 둘 다 "트리 탐색 UX 개선"이라는 같은 가치를 향하고, 사용자가 한 요청으로 묶어서 제시했다. 전체펼치기/접기는 단독으로도 가치가 있지만 모드 전환과 함께 쓰였을 때(모드를 바꾼 직후 새 트리를 한 번에 훑어보고 싶을 것) 더 자연스럽다 — `spec-viewer-editor-mode`가 리듀서 신호/키맵/main 배선을 한 스펙으로 묶었던 것과 같은 결.
- **Rejected alternatives**:
  - *두 개 스펙으로 분리*: 전체펼치기/접기는 순수 `TreeState` 조작이라 훨씬 단순하고 모드 전환과 기술적으로 독립적이긴 하다. 하지만 사용자가 한 요청으로 제시했고 공유하는 키맵/문서 영역이 겹쳐(둘 다 "트리 탐색 키" 도움말 섹션에 들어감) 분리 이득이 적다고 판단.

## Scope
- **In**: brief.md의 Scope In 그대로.
- **Out**: brief.md의 Scope Out 그대로.

## Constraints
brief.md 그대로: 1단계 펼침/접힘 무회귀, 전체펼치기/접기는 명시적 액션, `--all`은 초기값으로 유지, 읽기 전용.

## Boundary Strategy
- **Why this split**: `app/mod.rs`(리듀서 신호)/`app/keymap.rs`(키 바인딩)/`main.rs`(감시 재시작 배선)로 나누는 것이 이미 두 번(`spec-viewer-editor-mode`, 구조적으로 `spec-viewer-spec-kit-support`) 검증된 이 코드베이스의 표준 레이어 분리다.
- **Shared seams to watch**: 모드 전환이 "원래 시작 경로"를 어떻게 기억/재사용할지가 design 단계의 핵심 결정 지점 — `AppState`에 필드를 추가하면 기존 생성자(`AppState::new`) 호출부(main.rs, 다수의 테스트)에 영향을 준다(`spec-viewer-spec-kit-support`의 `Spec` 필드 추가가 기존 테스트를 대량으로 건드렸던 것과 같은 종류의 리스크) — design 단계에서 이 리스크를 명시하고, tasks 단계에서 전체 회귀 스위트를 자주 돌리도록 명시해야 한다.

## Specs (dependency order)
- [ ] spec-viewer-tree-navigation-modes -- 실행 중 모드 전환 + 전체펼치기/전체접기. Dependencies: none
