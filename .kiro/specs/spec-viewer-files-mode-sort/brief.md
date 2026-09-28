# Brief: spec-viewer-files-mode-sort

## Problem
`--all` 모드(`TreeSource::Files`, 순수 마크다운 디렉터리 트리)에서 `s`(정렬 순환) 키를 눌러도 아무 일도 일어나지 않는다. `state.sort_key` 값 자체는 바뀌지만 실제로 트리를 재정렬하는 코드가 없어 조용히 무시된다 — 숨은 버그에 가깝다.

## Current State
`app::mod::cycle_sort`가 `if let TreeSource::Kiro(root) = &mut state.root { spec::sort_specs(...) }`로 감싸져 있어 `Files` 모드에서는 아무 것도 하지 않는다. `spec::fs_tree::FsTree::scan`은 `entries`를 경로 문자열 순으로 한 번만 정렬하고(`entries.sort_by(|a,b| a.path.cmp(&b.path))`), `FsEntry`는 `path`/`is_dir`/`depth`만 가져 수정시각이 없다. `ui::tree_panel`의 Files 모드 타이틀은 고정 문자열 `"Files"`로, 정렬 키 표시가 없다.

## Desired Outcome
`--all` 모드에서 `s` 키를 누르면 "이름 ↔ 최근 수정" 2단으로 정렬이 순환하고, 실제로 트리의 형제 노드 순서가 바뀌며, 패널 타이틀에 현재 정렬 키가 표시된다. `.kiro` 모드의 기존 4단 순환(이름/phase/최근 갱신/진행률)은 전혀 바뀌지 않는다.

## Approach
- **선택**: `FsEntry`에 수정시각 필드를 추가해 `FsTree::scan`이 함께 읽어두고, `FsTree`에 정렬 키를 받아 형제 그룹별로 재정렬하는 함수를 추가한다(트리 중첩 구조 자체는 유지 — 전체를 한 번에 뒤섞는 flat 정렬이 아니라 각 디렉터리 레벨 안에서만 순서를 바꿈). `cycle_sort`가 `Files` 모드에서도 이 함수를 호출하도록 확장하고, `SortKey`에 "이 모드에서 순환 가능한 다음 키"를 판단하는 로직을 추가해 `Files` 모드에서는 Phase/Progress를 건너뛴다.
- **이유**: `build_files_items`/`flatten_files`가 "정렬된 경로 순서 = 올바른 트리 중첩"이라는 전제에 의존하므로(design.md 확인 필요), 이 전제를 깨지 않는 재정렬 방식이어야 새 트리 재구성 로직을 만들지 않고 기존 렌더링/검색 코드를 그대로 재사용할 수 있다.

## Scope
- **In**: `--all` 모드의 이름/최근 수정 정렬, `s` 키 순환 확장, 정렬 순환에서 모드별로 유효한 키만 도는 것, Files 모드 패널 타이틀에 정렬 키 표시.
- **Out**: `.kiro`/spec-kit 모드의 정렬 로직 변경, `SortKey`에 새 값 추가, 정렬 방향(오름차순/내림차순) 토글 같은 추가 UX, `--sort` CLI 플래그가 받는 값 목록 확장(플래그 자체는 이번 스펙 범위 밖 — 설계 단계에서 재확인).

## Boundary Candidates
- `src/spec/fs_tree.rs` — `FsEntry`/`FsTree`의 데이터 모델과 정렬 로직
- `src/spec/sort.rs` — `SortKey`에 "이 소스에서 유효한 키인가/다음 키" 판단 로직 추가 여부
- `src/app/mod.rs` — `cycle_sort`가 `Files` 모드에서도 동작하도록 확장
- `src/ui/tree_panel.rs` — Files 모드 타이틀에 정렬 키 표시

## Out of Boundary
- spec-kit(`.specify/`) 모드는 이미 정렬 개념이 없고(design.md 기존 결정), 이번 스펙도 건드리지 않는다.

## Upstream / Downstream
- **Upstream**: 원 `spec-viewer` 스펙의 `FsTree`/정렬 기능, `spec-viewer-kiro-folder-groups`가 `FsTree`/`build_files_items`/`flatten_files`를 재사용해 만든 `.kiro` 그룹 기능(이 재정렬이 그룹 트리에도 영향을 주는지 확인 필요).
- **Downstream**: 없음(순수 표시/정렬 개선).

## Existing Spec Touchpoints
- **Extends**: `spec-viewer`(원본, `FsTree`/`SortKey`/`cycle_sort` 정의).
- **Adjacent**: `spec-viewer-kiro-folder-groups`(그룹도 내부적으로 `FsTree`를 쓰므로, 이번 재정렬 함수가 그룹 트리에도 자연히 적용되는지 설계 단계에서 확인).

## Constraints
- `build_files_items`/`flatten_files`가 의존하는 "경로 정렬 = 올바른 트리 중첩" 전제를 깨면 안 된다 — 재정렬은 형제 단위로만 이뤄져야 한다.
- 기존 `.kiro`/spec-kit 모드의 정렬 동작과 회귀 없어야 한다.
