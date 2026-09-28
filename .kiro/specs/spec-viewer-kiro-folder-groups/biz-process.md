# BizProcess — spec-viewer-kiro-folder-groups

## 정의
개발자를 위해 `.kiro` 프로젝트 안의 `specs` 외 문서 폴더(steering, reference, guide 등)도 spec-viewer 트리에서 하위 폴더까지 자동으로 찾아 탐색하는 사용자 시각 프로세스이다.

## 가치사슬 매핑

| L1 Process | Unit Process (valueChainRef) | 가치 | 관련 요구사항 |
|------------|------------------------------|------|---------------|
| BP-KIRO-GROUPS | VC-DEV-SPEC-VIEW | 터미널에서 스펙·문서를 읽기 전용으로 빠르게 파악한다(specs 밖에 흩어진 참고 문서까지 같은 트리에서 파악하게 해 "빠르게 파악" 가치를 강화) | 1 ~ 5 |

### L2/L3 ↔ 요구사항 추적

| 노드 | 이름 | 관련 요구사항 |
|------|------|---------------|
| L2-C1 | 폴더 자동 발견 | 1.1~1.6 |
| L2-C2 | 하위 폴더까지 탐색 | 2.1~2.4 |
| L2-C3 | steering 표시 방식 무회귀 | 3.1~3.2 |
| L2-C4 | 기존 기능과의 통합 | 4.1~4.5 |
| L2-C5 | 모드 경계 | 5.1 |
| L3-G1 | 트리 패널(그룹 노드) | 1.1~1.6, 2.1~2.4 |
| L3-G2 | 트리 패널(steering 배지) | 3.1~3.2 |
| L3-G3 | 문서 패널/검색/편집/상태 표시줄 | 4.1~4.5 |
| L3-G4 | (화면 없음 — 결과로만 관찰) | 5.1 |

---

## L1 Process: `.kiro` 폴더 자동 발견과 탐색  (id: BP-KIRO-GROUPS, valueChainRef: VC-DEV-SPEC-VIEW)

개발자가 spec-viewer로 `.kiro` 프로젝트를 열었을 때, `specs` 폴더뿐 아니라 자신이 만든 다른 문서 폴더(steering, reference, guide 등)도 같은 트리 안에서 하위 폴더까지 펼쳐가며 찾아본다.

```
L2-C1 Activity: 폴더 자동 발견  (1.1~1.6)
  L3-G1 UI: 트리 패널(그룹 노드)
    L4-S1 Step: spec-viewer가 `.kiro` 트리를 연다
      L5-D1 DetailStep: `specs`가 아닌 하위 폴더를 훑어본다
        Logic(AST):
          - IF 그 폴더 안(하위 폴더 포함)에 마크다운 문서가 하나 이상 있음 THEN 그 폴더 이름으로 그룹이 트리에 나타난다(1.1)
          - ELSE 그 폴더는 트리에 나타나지 않는다(1.2)
      L5-D2 DetailStep: 그런 폴더가 여럿이다
        Logic(AST):
          - 모든 폴더가 각자의 그룹으로 동시에 나타난다(1.3)
          - 그룹들은 실행마다 같은 순서로 보인다(1.5)
      L5-D3 DetailStep: 항상
        Logic(AST):
          - `specs`는 이 발견 대상이 아니고 기존 스펙 트리로만 보인다(1.4)
      L5-D4 DetailStep: 발견될 폴더가 없다
        Logic(AST):
          - 기존과 동일하게 스펙 트리만 보이고 오류가 없다(1.6)

L2-C2 Activity: 하위 폴더까지 탐색  (2.1~2.4)
  L3-G1 UI: 트리 패널(그룹 노드)
    L4-S2 Step: 그룹 노드나 그 하위 폴더 노드를 펼치거나 접는다
      L5-D5 DetailStep: 그룹 폴더 안에 하위 폴더가 있다
        Logic(AST):
          - 펼쳐서 안의 문서를 본다(2.1), 몇 단계든 계속 들어갈 수 있다(2.2)
          - `steering`도 다른 그룹과 동일하게 하위 폴더를 펼쳐볼 수 있다(2.3)
      L5-D6 DetailStep: 항상
        Logic(AST):
          - 펼치거나 접는 동작은 그 노드의 직계 한 단계에만 적용되고 다른 단계까지 함께 바뀌지 않는다(2.4)

L2-C3 Activity: steering 표시 방식 무회귀  (3.1~3.2)
  L3-G2 UI: 트리 패널(steering 배지)
    L4-S3 Step: 그룹 안 문서를 살펴본다
      L5-D7 DetailStep: `steering` 폴더 바로 아래 문서다
        Logic(AST):
          - 기존과 동일하게 포함 방식(always/manual/fileMatch/auto) 표시가 함께 보인다(3.1)
      L5-D8 DetailStep: `steering`이 아닌 그룹의 문서다
        Logic(AST):
          - 포함 방식 표시 없이 일반 문서로 보인다(3.2)

L2-C4 Activity: 기존 기능과의 통합  (4.1~4.5)
  L3-G3 UI: 문서 패널/검색/편집/상태 표시줄
    L4-S4 Step: 그룹 안 문서를 다루듯 기존 기능을 그대로 쓴다
      L5-D9 DetailStep: 문서를 선택한다
        Logic(AST):
          - 문서 패널에 렌더링된 내용이 보인다(4.1)
      L5-D10 DetailStep: 검색·편집·감시를 쓴다
        Logic(AST):
          - 이름 검색·내용 검색이 스펙·steering 문서와 동일하게 동작한다(4.2)
          - 편집 키로 외부 에디터가 열리고 저장이 반영된다(4.3)
          - 전체펼치기/전체접기 키가 그룹과 하위 폴더에도 함께 적용된다(4.4)
          - 파일 감시 중 그룹 폴더 아래 변경이 생기면 트리·문서가 자동 갱신된다(4.5)

L2-C5 Activity: 모드 경계  (5.1)
  L3-G4 UI: (화면 없음 — 개발자는 결과로만 관찰)
    L4-S5 Step: GitHub spec-kit 프로젝트를 연다
      L5-D11 DetailStep: 항상
        Logic(AST):
          - 이 기능의 영향을 받지 않고 spec-kit 트리 표시는 그대로다(5.1)
```

### ✅ 검토 요청 (L1: `.kiro` 폴더 자동 발견과 탐색)
승인(✓) 또는 수정 사항을 입력하세요.
