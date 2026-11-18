"""Validate native feedback payloads emitted by the actual provider adapter."""
import base64
import hashlib
import json
from pathlib import Path


def receive(provider, content, output, probe):
    texts=[]
    images=[]
    for item in content:
        if item.get('type') in ('text','inputText'):
            texts.append(item.get('text',''))
        elif item.get('type')=='image':
            source=item.get('source',{})
            if source.get('type')=='base64':
                images.append(base64.b64decode(source['data'],validate=True))
        elif item.get('type')=='localImage':
            images.append(Path(item['path']).read_bytes())
    text='\n'.join(texts)
    result={'probe':probe,'provider':provider,'success':False,'scenario':'annotations'}
    try:
        start='<ferrite_browser_annotations version="1">\n'
        body=text.split(start,1)[1].split('\n</ferrite_browser_annotations>',1)[0]
        annotations=json.loads(body)
        if len(annotations)!=2:raise ValueError('Batch must contain exactly two retained annotations after removal')
        if annotations[0]['comment']!='Packaged annotation comment':raise ValueError('Drawn feedback comment missing')
        if annotations[1]['comment']!='Annotated fixture element':raise ValueError('Element feedback comment missing')
        if any(annotation['capture']['selection']['kind']!='live' for annotation in annotations):raise ValueError('In-page editor capture missing')
        feedback=[annotation['capture']['selection']['feedback'] for annotation in annotations]
        if not feedback[0]['strokes']:raise ValueError('Drawn stroke did not survive delivery')
        if not any(target['element']['tagName']=='button' for target in feedback[1]['elements']):raise ValueError('Picked fixture element did not survive delivery')
        if any(item['screenshot'].get('kind')!='captured' for item in feedback):raise ValueError(f'Screenshot crop missing: {[item["screenshot"] for item in feedback]}')
        expected={annotation['original_sha256'] for annotation in annotations}
        if None in expected:raise ValueError('Annotation lost its retained original')
        actual={hashlib.sha256(png).hexdigest() for png in images if png.startswith(b'\x89PNG\r\n\x1a\n')}
        if actual!=expected:raise ValueError('Provider did not receive the original native PNG bytes')
        for index,png in enumerate(images):(output/f'annotation-provider-{index}.png').write_bytes(png)
        result.update(success=True,annotations=annotations,image_hashes=sorted(actual),image_count=len(images))
    except (ValueError,KeyError,IndexError,TypeError) as error:
        result.update(error=str(error),text=text,image_count=len(images))
    (output/'provider-result.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
