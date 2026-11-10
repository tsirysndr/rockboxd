module Native.File where
import Effect (Effect)
import Prelude
import Native.Types
import Native.Engine as E
import CI.Support as S
import Effect.Class (liftEffect)
contents :: File -> Effect String
contents file = liftEffect (E.materializeFile file >>= E.readExact)
