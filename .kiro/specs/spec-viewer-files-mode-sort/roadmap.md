# Roadmap

## Overview
`--all` 모드에서 조용히 무시되던 정렬 키(`s`)를 실제로 동작하게 만든다: 이름/최근 수정 2단 순환, 형제 단위 재정렬로 기존 트리 중첩 전제를 지키면서, `.kiro` 모드의 4단 순환은 그대로 둔다.

## Approach Decision
- **Chosen**: 단일 스펙(Path 분해 없음). 데이터 모델(`FsEntry` mtime), 정렬 로직(형제 단위 재정렬), 배선(`cycle_sort`), 표시(타이틀)를 하나의 스펙으로 순서대로 다룬다.
- **Why**: 모두 "정렬 키 하나를 Files 모드에 연결하는" 단일 목표에서 파생되는 downstream 반영이라 인위적으로 쪼갤 이유가 없다 — `spec-viewer-kiro-folder-groups`의 선례와 같은 판단.
- **Rejected alternatives**:
  - *별도 CLI 플래그/정렬 방향 토글까지 포함*: 사용자가 명시적으로 범위 밖(Out)으로 뒀다 — 이번 스펙은 기존 `s` 키 순환에 Files 모드를 얹는 것에 집중.

## Scope
- **In**: brief.md의 Scope In 그대로.
- **Out**: brief.md의 Scope Out 그대로.

## Constraints
brief.md 그대로: 형제 단위 재정렬(트리 중첩 전제 유지), `.kiro`/spec-kit 무회귀.

## Boundary Strategy
- **Why this split**: `fs_tree.rs`(데이터+정렬 알고리즘) → `sort.rs`(모드별 유효 키 판단) → `app/mod.rs`(배선) → `tree_panel.rs`(표시)로 이어지는 단방향 의존 순서가 이미 이 코드베이스의 레이어 구조(`spec` → `app` → `ui`)와 일치한다.
- **Shared seams to watch**: `spec-viewer-kiro-folder-groups`의 `KiroGroup.tree`도 같은 `FsTree` 타입을 쓴다 — 이번에 추가하는 형제 단위 재정렬 함수가 그룹 트리에도 자동으로 적용될지, 아니면 `.kiro` 그룹은 정렬 개념에서 명시적으로 제외할지 design 단계에서 결정해야 한다(그룹은 이번 스펙의 Scope에 없으므로 기본값은 "영향 없음/그대로"여야 하며, 실제로 그런지 확인 필요).

## Specs (dependency order)
- [ ] spec-viewer-files-mode-sort -- `--all` 모드 이름/최근 수정 정렬 + 정렬 키 표시. Dependencies: none
