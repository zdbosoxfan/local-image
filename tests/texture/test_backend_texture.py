"""Run persisted-layer regressions with the actual replacement healer and API."""
from pathlib import Path
import sys
import unittest

HERE=Path(__file__).resolve().parents[2]/'backend'
sys.path[:0]=[str(HERE),str(HERE.parent/'tests'/'helpers')]
import fast_inpaint
assert Path(fast_inpaint.__file__).resolve().parent == HERE
import backend_folder_save_test as previous
previous.V3=HERE

class CurrentBackendTests(previous.FolderSaveTests):
    async def test_texture_default_and_explicit_dust_route_preserve_layers(self):
        source=self.make_image(size=(80,64))
        session=self.bind(source)
        for expected,method in [('Texture repair','texture'),('Dust & scratches','telea')]:
            payload=self.fixture.remove_payload('heal')
            payload.revision=session['revision']
            payload.heal_method=method
            session=await self.app.remove(session['id'],self.request(),payload)
            self.assertEqual(session['layers'][-1]['heal_method'],method)
            self.assertEqual(session['layers'][-1]['model_label'],expected)
            self.assertEqual(session['layers'][-1]['model'],'heal')
        self.assertEqual(len(session['layers']),2)
        self.assertEqual(self.app.RemoveRequest(mask='unused',revision=0,model='heal').heal_method,'texture')

if __name__=='__main__':
    unittest.main(verbosity=2)
