# dennett-sensor-worker

Изолированный runtime умного microphone/screen/camera/clipboard capture. Производит Ambient Candidates, а не готовую долговременную память.

Должен уметь остановиться локально, соблюдать exclusions и pressure policy, не отправлять постоянный raw stream тяжёлой модели.
