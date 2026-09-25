# Implementation Plan — spec-viewer-spec-kit-support

## 정의
spec-viewer 구현자를 위해 design.md 컴포넌트 경계(`spec`/`spec::spec_kit`/`ui::tree_panel`/`main`)를 따라 관찰 가능한 DONE 상태 단위로 분해한 구현 계획이다.

- [ ] 1. 소스 무관 진행 배지 모델
- [ ] 1.1 `Milestone`/`Spec` 확장과 `.kiro` 마일스톤 계산 규칙
  - DONE: `Spec`에 `milestones: Vec<Milestone>`/`warning: Option<String>`/`kiro_meta: Option<Result<SpecMeta, MetaError>>`가 추가되고, `.kiro` 빌더가 `approvals` 맵의 각 키를 마일스톤으로 변환하며, `phase` 값이 정확히 `"completed"`일 때만 "구현 완료" 마일스톤을 추가로 붙인다는 것을 단위 테스트로 확인. 같은 4개 게이트가 전부 승인된 두 스펙(하나는 phase가 `completed`, 하나는 아님)이 서로 다른 `milestones.len()`(5 vs 4)을 갖는 것으로 "게이트 전부 승인"과 "구현 완료"가 실제로 구분됨을 확인(지난 세션 반려 사유의 직접적 재발 방지 검증). `spec.json` 파싱 실패/부재 시 `warning`에 메시지가 담기고 `milestones`는 빈 벡터가 되는 것도 확인. 기존 `.kiro` 전용 테스트들이 새 필드를 포함하도록 갱신되어 전체 빌드가 통과.
  - _Requirements: 3.1, 3.2, 3.3, 3.4, 5.1, 5.2, 5.3, 5.4, 5.5_
  - _Boundary: spec (Spec, Milestone, TreeSource)_
  - _Difficulty: high_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B3_
- [ ] 1.2 정렬 로직의 `kiro_meta` 경유 갱신
  - DONE: `SortKey::Phase`/`SortKey::Updated` 정렬이 `spec.meta` 대신 `spec.kiro_meta`를 통해 동일하게 동작하는 것을 기존 정렬 테스트로 재확인(리네임만, 정렬 순서 자체는 무회귀).
  - _Requirements: 5.1_
  - _Boundary: spec::sort_
  - _Difficulty: low_
  - _Depends: 1.1_

- [ ] 2. spec-kit 루트 인식과 파싱
- [ ] 2.1 (P) `.specify/` 루트 탐지와 소스 우선순위 판정
  - DONE: `find_spec_kit_root`가 `.kiro::find_root`와 같은 상향 탐색 방식으로 `.specify/`를 찾되 그 부모 디렉터리를 반환하는 것, 그리고 같은 디렉터리에 `.kiro/`와 `.specify/`가 공존할 때 `.kiro` 우선이라는 판정 규칙(순수 함수 단위)을 단위 테스트로 확인.
  - _Requirements: 1.1, 1.2, 1.3_
  - _Boundary: spec::spec_kit (find_spec_kit_root)_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B1_
- [ ] 2.2 spec-kit 기능 디렉터리 파싱과 마일스톤 계산
  - DONE: `specs/<NNN-이름>/` 각각이 `Spec`으로 변환되고, `spec.md`/`plan.md`/`tasks.md` 존재 여부가 각각 하나의 마일스톤으로, 캐노니컬 순서(`spec.md → plan.md → tasks.md → research.md → data-model.md → quickstart.md → contracts/*`)로 존재하는 문서만 `DocEntry(kind: Other, status: NotTracked)`로 나열되고 순서 밖 파일은 끝에 추가되는 것, `tasks.md` 슬롯만 `count_progress`로 `Progress`가 채워지는 것, `specs/`가 없거나 기능 디렉터리가 비어 있어도 오류 없이 빈 결과가 나오는 것을 단위 테스트로 확인. 어떤 테스트에서도 대상 디렉터리에 파일이 생성/수정/삭제되지 않음을 확인.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 4.1, 4.2, 4.3, 4.4, 4.5, 6.1, 6.2, 8.1_
  - _Boundary: spec::spec_kit (build)_
  - _Difficulty: high_
  - _Depends: 1.1_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B2_

- [ ] 3. 공유 배지 렌더링과 트리 표시
- [ ] 3.1 (P) 공유 `milestone_badge_spans` 함수로 교체
  - DONE: 기존 `spec_item`(`.kiro`)의 배지 계산 부분이 `milestones`/`warning`을 받는 공유 함수 호출로 바뀌고, 렌더된 프레임에서 기존 `.kiro` 배지 표시(예: 승인 게이트 개수, 경고 `!`)가 리팩터링 전과 동일하게 보이는 것을 기존 렌더 테스트로 재확인(무회귀). 마일스톤이 비어 있으면 배지 없이 이름만, 전부 완료면 굵게+녹색 강조가 나오는 것도 렌더 버퍼로 확인.
  - _Requirements: 3.1, 3.2, 3.3, 3.4_
  - _Boundary: ui::tree_panel (milestone_badge_spans, spec_item)_
  - _Difficulty: mid_
  - _Depends: 1.1_
- [ ] 3.2 spec-kit 기능 트리 렌더링
  - DONE: `TreeSource::SpecKit`을 받아 `feature_item`을 나열하는 렌더 경로가 추가되고(Steering 그룹 없음), 실행 인자로 지정된 spec-kit 프로젝트를 열었을 때 렌더된 프레임에 기능 노드 이름·마일스톤 배지·펼쳤을 때의 문서 자식 노드가 실제로 보이는 것을 렌더 버퍼 검증으로 확인.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 3.1, 3.2, 3.3, 3.4_
  - _Boundary: ui::tree_panel (feature_item, render)_
  - _Difficulty: mid_
  - _Depends: 2.2, 3.1_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B2, BP-SPEC-KIT-VIEW.L2-B3_

- [ ] 4. `main`의 소스 우선순위 연결과 감시 루트 배선
  - DONE: `resolve_source`가 `--all` 최우선 → 같은 디렉터리 공존 시 `.kiro` 우선 → 더 가까운 상위 마커 순으로 `TreeSource`(`Files`/`Kiro`/`SpecKit`)를 고르는 것을 통합 테스트로 확인(다섯 조합: `.kiro`만/`.specify`만/둘 다/`--all`/둘 다 없음 각각의 기대 결과). spec-kit 모드에서 파일 감시가 프로젝트 루트 전체가 아니라 `specs/`만을 대상으로 시작되는 것도 확인. 아무 마커도 없을 때 기존과 동일한 탐색 실패 오류·종료 코드가 나오는 것으로 무회귀 확인.
  - _Requirements: 1.1, 1.2, 1.3, 1.4_
  - _Boundary: main (resolve_source)_
  - _Difficulty: high_
  - _Depends: 2.1, 2.2, 3.2_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B1_

- [ ] 5. E2E 검증
- [ ] 5.1 `.kiro` 무회귀와 완료 상태 구분 실물 확인
  - DONE: 이 스펙 작업 전부터 있던 `.kiro` 전체 회귀 스위트가 100% 통과하고, 실제로 렌더된 두 프레임(하나는 게이트 전부 승인·`phase`가 `completed`가 아닌 스펙, 다른 하나는 `phase: completed`인 스펙)을 나란히 비교해 배지 표시가 서로 다르게 나온다는 것을 실물 확인.
  - _Requirements: 5.1, 5.2, 5.3, 5.5_
  - _Depends: 4_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B3_
- [ ] 5.2 (P) `bugfix.md` 전용 경로 정확 추적 확인
  - DONE: `approvals`에 표준 게이트 중 일부(예: `bugfix`, `tasks`)만 기록된 `.kiro` 스펙이 그 개수 그대로(`n/2`처럼) 표시되고 존재하지 않는 게이트가 있는 것처럼 보이지 않는 것을 렌더 버퍼로 확인.
  - _Requirements: 5.4_
  - _Depends: 4_
  - _Difficulty: low_
- [ ] 5.3 spec-kit 임시 프로젝트 E2E: 트리 탐색과 문서 조회
  - DONE: 임시 spec-kit 프로젝트 픽스처(`tests/fixtures/spec-kit/`, 산출물 진행 정도가 서로 다른 기능 2개 이상 포함)를 열어 기능 트리를 펼치고, `spec.md`/`plan.md`를 선택하면 마크다운(다이어그램 포함)으로, `contracts/` 하위 파일을 선택하면 일반 텍스트로 문서 패널에 렌더되는 것을 실물 확인. `tasks.md`에 체크박스가 있는 기능은 `n/m` 진행률이, 없는 기능은 "진행률 없음"이 보이는 것도 확인.
  - _Requirements: 2.1, 2.2, 2.3, 2.4, 2.5, 6.1, 6.2, 7.1, 7.2_
  - _Depends: 4_
  - _Difficulty: mid_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B2, BP-SPEC-KIT-VIEW.L2-B4, BP-SPEC-KIT-VIEW.L2-B5_
- [ ] 5.4 (P) 편집모드 자연 연동 확인
  - DONE: spec-kit 문서(`spec.md` 등)가 문서 패널에 표시된 상태에서 편집 키를 누르면, 추가 구현 없이 기존 편집모드 파이프라인(`Action::Edit` → `Control::EditFile` → `handle_edit_file`)이 그대로 트리거되는 것을 확인.
  - _Requirements: 9.1_
  - _Depends: 4_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B7_
- [ ] 5.5 (P) 읽기 전용 불변 확인
  - DONE: spec-kit 임시 프로젝트를 열어 트리 탐색·문서 선택·정렬을 두루 수행한 뒤에도 `.specify/`·`specs/` 아래 파일의 mtime/내용이 시작 전과 완전히 동일하다는 것을 확인.
  - _Requirements: 8.1_
  - _Depends: 4_
  - _Difficulty: low_
  - _BizProcess: BP-SPEC-KIT-VIEW.L2-B6_
