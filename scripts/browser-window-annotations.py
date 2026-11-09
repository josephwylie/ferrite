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
        if annotations[0]['comment']!='Packaged annotation comment':raise ValueError('Original comment missing')
        if annotations[1]['comment']!='Immutable element after navigation and close':raise ValueError('Closed-page feedback missing')
        kinds={drawing['kind'] for drawing in annotations[0]['drawings']}
        if any(drawing['ink']!={'color':'#8cb9f4','width':6.0} for drawing in annotations[0]['drawings']):raise ValueError('Selected Blue ink/6px width did not survive delivery')
        if kinds!={'pen','line','arrow','rectangle','ellipse','text','step'}:raise ValueError(f'Actual drawing controls missing: {kinds}')
        if not any(drawing.get('text')=='Fixture label' for drawing in annotations[0]['drawings']):raise ValueError('Text label missing')
        if annotations[0]['capture']['selection']['kind']!='region' or annotations[1]['capture']['selection']['kind']!='element':raise ValueError('Region/element capture missing')
        expected={annotation['original_sha256'] for annotation in annotations}
        actual={hashlib.sha256(png).hexdigest() for png in images if png.startswith(b'\x89PNG\r\n\x1a\n')}
        if actual!=expected:raise ValueError('Provider did not receive the original native PNG bytes')
        for index,png in enumerate(images):(output/f'annotation-provider-{index}.png').write_bytes(png)
        result.update(success=True,annotations=annotations,image_hashes=sorted(actual),image_count=len(images))
    except (ValueError,KeyError,IndexError,TypeError) as error:
        result.update(error=str(error),text=text,image_count=len(images))
    (output/'provider-result.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
