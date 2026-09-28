# Roadmap

## Overview
`.kiro` 모드의 트리가 `specs`/`steering`만 하드코딩해서 보여주던 것을, `specs`를 제외한 모든 하위 폴더 중 `.md`가 있는 폴더를 자동 발견해 그룹으로 보여주도록 일반화한다. `steering`을 포함한 모든 그룹은 `--all` 모드의 `FsTree`(재귀 트리)를 재사용해 하위 폴더까지 펼쳐볼 수 있게 한다.

## Approach Decision
- **Chosen**: 단일 스펙(Path 분해 없음). 데이터 모델(`SpecRoot`/`NodeId`) 일반화, 스캔(`loader`), 렌더링(`tree_panel`), 순회(`search::flatten_tree`), 문서 선택(`app::mod`)을 하나의 스펙 안에서 순서대로 다룬다.
- **Why**: 모든 변경이 "같은 데이터 모델 일반화 하나"에서 파생되는 downstream 반영이라 인위적으로 쪼갤 이유가 없다 — `SpecRoot`를 바꾸는 순간 나머지는 그 타입을 따라가는 컴파일 타임 강제 변경이다.
- **Rejected alternatives**:
  - *스캔/데이터모델과 렌더링을 별도 스펙으로 분리*: 데이터 모델만 바꾸고 렌더링을 나중에 하면 중간 상태에서 컴파일이 깨지거나(`SpecRoot.steering` 필드 삭제가 `tree_panel.rs`를 즉시 깨뜨림) 죽은 코드가 남는다 — 분리 이득이 없음.

## Scope
- **In**: brief.md의 Scope In 그대로.
- **Out**: brief.md의 Scope Out 그대로 (spec-kit 모드는 범위 밖, 설계 단계에서 재확인).

## Constraints
brief.md 그대로: `steering`의 `inclusion` 배지 무회귀, `NodeId`의 트레잇 요구사항 유지, 이중 스캔 금지.

## Boundary Strategy
- **Why this split**: 기존 코드베이스가 이미 두 가지 트리 모델을 갖고 있다 -- `TreeSource::Kiro`(specs+steering, 지금까지 평평/구조화 혼합)와 `TreeSource::Files`(순수 재귀 `FsTree`). 이번 스펙의 핵심은 "Kiro 모드 안에 Files 모드의 재귀 트리 조각을 끼워 넣는 것"이므로, `FsTree`/`NodeId::Dir`/`NodeId::File`을 그대로 재사용하는 것이 SSoT/Simplification 둘 다에 부합한다.
- **Shared seams to watch**: `NodeId::Dir`/`File`이 지금까지는 오직 `TreeSource::Files`에서만 나왔는데, `TreeSource::Kiro` 트리 안에도 나타나게 되면 `resolve_selection`/`current_editable_path`/mouse 클릭/검색 등 "이 NodeId가 어느 TreeSource에서 왔는지 안다"고 암묵적으로 가정했을 수 있는 모든 코드를 재검토해야 한다 — design 단계에서 이 가정이 실제로 있는지 코드베이스 전체를 확인하고 명시해야 한다.

## Specs (dependency order)
- [ ] spec-viewer-kiro-folder-groups -- `.kiro` 하위 폴더 자동 발견 + 재귀 트리 표시(steering 포함). Dependencies: none
