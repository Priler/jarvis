<!-- Based on: https://github.com/rembertdesigns/Iron-Man-Arc-Reactor-Pure-CSS and https://codepen.io/FlyingEmu/pen/DZNqEj -->
<div id="arc-reactor" class="reactor-container">
    <div class="reactor-container-inner circle abs-center">
        <ul class="marks"><li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li>
            <li></li><li></li><li></li><li></li><li></li><li></li></ul>
        <div class="e7">
            <div class="semi_arc_3 e5_1">
                <div class="semi_arc_3 e5_2">
                    <div class="semi_arc_3 e5_3">
                        <div class="semi_arc_3 e5_4" />
                    </div>
                </div>
            </div>
        </div>
    </div>
    <div class="tunnel circle abs-center" />
    <div class="core-wrapper circle abs-center" />
    <div class="core-outer circle abs-center" />
    <div class="core-inner circle abs-center" />
    <div class="coil-container">
        <div class="coil coil-1" />
        <div class="coil coil-2" />
        <div class="coil coil-3" />
        <div class="coil coil-4" />
        <div class="coil coil-5" />
        <div class="coil coil-6" />
        <div class="coil coil-7" />
        <div class="coil coil-8" />
    </div>
</div>

<style lang="scss" global>
    $arc_radius: 130px;
    $size3: 6px;
    $cshadow: rgba(2, 254, 255, 0.8);
    $marks_color_1: rgba(2, 254, 255, 1);
    $marks_color_2: rgba(2, 254, 255, 0.3);
    $colour1: rgba(2, 255, 255, 0.15);
    $colour3: rgba(2, 255, 255, 0.3);
    $cshadow: rgba(2, 254, 255, 0.8);

    .reactor-container {
        width: 300px;
        height: 320px;
        margin: auto;
        //  border: 1px dashed #888;
        position: relative;
        border-radius: 50%;
        transition: scale 1s ease, opacity .5s ease;
        scale: 0.9;
        opacity: .9;
        //   background-color: #384c50;
        //   border: 1px solid #121414;
        //   box-shadow: 0px 0px 32px 8px #121414, 0px 0px 4px 1px #121414 inset;

        ul {
            list-style: none;
            margin: 0;
            padding: 0;
        }
    }
    .reactor-container-inner {
        height: 238px;
        width: 238px;
        background-color: #161a1b;
        box-shadow: 0px 0px 50px 15px $colour3, inset 0px 0px 50px 15px $colour3;
        // box-shadow: 0px 0px 4px 1px #52fefe;
    }
    .circle {
        border-radius: 50%;
    }
    .abs-center {
        position: absolute;
        top: 0;
        right: 0;
        bottom: 0;
        left: 0;
        margin: auto;
    }
    .core-inner {
        width: 70px;
        height: 70px;
        border: 5px solid #1b4e5f;
        background-color: #ffffff;
        box-shadow: 0px 0px 7px 5px #52fefe, 0px 0px 10px 10px #52fefe inset;
    }
    .core-outer {
        width: 120px;
        height: 120px;
        border: 1px solid #52fefe;
        background-color: #ffffff;
        box-shadow: 0px 0px 2px 1px #52fefe, 0px 0px 10px 5px #52fefe inset;
    }
    .core-wrapper {
        width: 180px;
        height: 180px;
        background-color: #073c4b;
        box-shadow: 0px 0px 5px 4px #52fefe, 0px 0px 6px 2px #52fefe inset;
    }
    .tunnel {
        width: 220px;
        height: 220px;
        background-color: #ffffff;
        box-shadow: 0px 0px 5px 1px #52fefe, 0px 0px 5px 4px #52fefe inset;
    }
    .coil-container {
        position: relative;
        width: 100%;
        height: 100%;
        animation: 10s infinite linear reactor-anim;
        transition: animation 1s;
    }
    .coil {
        position: absolute;
        width: 30px;
        height: 20px;
        top: calc(50% - 110px);
        left: calc(50% - 15px);
        transform-origin: 15px 110px;
        background-color: #073c4b;
        box-shadow: 0px 0px 5px #52fefe inset;
    }
    .coil-1 {
        transform: rotate(0deg);
    }
    .coil-2 {
        transform: rotate(45deg);
    }
    .coil-3 {
        transform: rotate(90deg);
    }
    .coil-4 {
        transform: rotate(135deg);
    }
    .coil-5 {
        transform: rotate(180deg);
    }
    .coil-6 {
        transform: rotate(225deg);
    }
    .coil-7 {
        transform: rotate(270deg);
    }
    .coil-8 {
        transform: rotate(315deg);
    }
    @keyframes reactor-anim {
        from {
            transform: rotate(0deg);
        }
        to {
            transform: rotate(360deg);
        }
    }

    @mixin border-radius($pixel...) {
        border-radius: $pixel;
    }

    .e7 {
        position: relative;
        z-index: 1;
        width: 160%;
        height: 160%;
        left: -32.5%;
        top: -32.5%;
        right: 0;
        bottom: 0;
        margin: auto;
        border: $size3 solid transparent;
        background: transparent;
        @include border-radius(50%);
        transform: rotateZ(0deg);
        transition: box-shadow 3s ease;
        text-align: center;
        opacity: .3;
    }

    .semi_arc {
        width: 100px;
        height: 100px;
        border: 6px solid #02feff;
        background: rgba(2, 254, 255, 0.2);
        -moz-border-radius: 50%;
        -webkit-border-radius: 50%;
        border-radius: 50%;
        transform: rotateZ(0deg);
        transition: box-shadow 3s ease;
        text-align: center;
        overflow: hidden;
    }

    .semi_arc:hover {
        box-shadow: 0px 0px 30px $cshadow;
        transition: 0.3s;
    }

    .semi_arc_2 {
        content: "";
        position: absolute;
        width: 94%;
        height: 94%;
        left: 3%;
        top: 3%;
        border: 5px solid #02feff;
        -moz-border-radius: 50%;
        -webkit-border-radius: 50%;
        border-radius: 50%;
        -moz-box-sizing: border-box;
        -webkit-box-sizing: border-box;
        box-sizing: border-box;
        animation: rotate 4s linear infinite;
        text-align: center;
        overflow: hidden;
    }

    .semi_arc_2:after {
        content: "";
        position: absolute;
        width: 94%;
        height: 94%;
        left: 3%;
        top: 3%;
        border: 4px solid #02feff;
        -moz-border-radius: 50%;
        -webkit-border-radius: 50%;
        border-radius: 50%;
        -moz-box-sizing: border-box;
        -webkit-box-sizing: border-box;
        box-sizing: border-box;
        animation: rotate_anti 2s linear infinite;
    }

    .semi_arc_3 {
        content: "";
        position: absolute;
        width: 94%;
        height: 94%;
        left: 3%;
        top: 3%;
        border: 5px solid #02feff;
        -moz-border-radius: 50%;
        -webkit-border-radius: 50%;
        border-radius: 50%;
        -moz-box-sizing: border-box;
        -webkit-box-sizing: border-box;
        box-sizing: border-box;
        animation: rotate 4s linear infinite;
        text-align: center;
        overflow: hidden;
    }

    .e1:after {
        border-color: rgba(2, 255, 255, 0.6);
        border-left: 5px solid transparent;
        border-right: 5px solid transparent;
    }

    .e2:after {
        border-color: rgba(2, 255, 255, 0.6);
        border-left: 5px solid transparent;
        border-right: 5px solid transparent;
        border-bottom: 5px solid transparent;
    }

    .e3 {
        border-left: 6px solid transparent;
        border-right: 6px solid transparent;
        animation: rotate 5s linear infinite;
    }

    .e3:after {
        border-color: rgba(2, 255, 255, 0.6);
        border-top: 5px solid transparent;
        border-bottom: 5px solid transparent;
    }

    .e4 {
        width: 150px;
        height: 150px;
    }

    .e4_1 {
        border-color: rgba(2, 255, 255, 0.3);
        border-left: 5px solid transparent;
        border-right: 5px solid transparent;
    }

    .e4_1:after {
        border-color: rgba(2, 255, 255, 0.6);
        border-top: 4px solid transparent;
        border-bottom: 4px solid transparent;
    }

    .e5 {
        width: 200px;
        height: 200px;
    }

    .e5_1 {
        color: rgba(2, 255, 255, 0.15);
        border: 2px solid;
        border-left: 2px solid transparent;
        animation: rotate 5s linear infinite;
    }

    .e5_2 {
        color: rgba(2, 255, 255, 0.7);
        border: 4px solid;
        border-left: 4px solid transparent;
        border-right: 4px solid transparent;
        animation: rotate_anti 4s linear infinite;
    }

    .e5_3 {
        color: rgba(2, 255, 255, 0.5);
        border: 2px solid;
        border-left: 2px solid transparent;
        border-right: 2px solid transparent;
        animation: rotate 3s linear infinite;
    }

    .e5_4 {
        color: rgba(2, 255, 255, 0.15);
        border: 4px solid;
        border-left: 4px solid transparent;
        border-right: 4px solid transparent;
        border-bottom: 4px solid transparent;
        animation: rotate_anti 2s linear infinite;
    }

    .e6 {
        border-color: transparent;
        background: rgba(255, 255, 255, 0);
        width: 200px;
        height: 200px;
    }

    @keyframes rotate_anti {
        0% {
            transform: rotateZ(360deg);
        }
        100% {
            transform: rotateZ(0deg);
        }
    }

    @keyframes rotate {
        0% {
            transform: rotateZ(0deg);
        }
        100% {
            transform: rotateZ(360deg);
        }
    }

    .marks {
        position: absolute;
        left: 0;
        top: 0;
        right: 0;
        bottom: 0;
        margin: auto;

        li {
            display: block;
            width: 3px;
            height: 11px;
            background: $cshadow;
            position: absolute;
            margin-left: 117.5px;
            margin-top: 113.5px;
            animation: colour_ease2 3s infinite ease-in-out;
        }
    }

    @keyframes colour_ease2 {
        0% {
            background: $marks_color_1;
        }
        50% {
            background: $marks_color_2;
        }
        100% {
            background: $marks_color_1;
        }
    }

    .marks li:first-child {
        transform: rotate(6deg) translateY($arc_radius);
    }
    .marks li:nth-child(2) {
        transform: rotate(12deg) translateY($arc_radius);
    }
    .marks li:nth-child(3) {
        transform: rotate(18deg) translateY($arc_radius);
    }
    .marks li:nth-child(4) {
        transform: rotate(24deg) translateY($arc_radius);
    }
    .marks li:nth-child(5) {
        transform: rotate(30deg) translateY($arc_radius);
    }
    .marks li:nth-child(6) {
        transform: rotate(36deg) translateY($arc_radius);
    }
    .marks li:nth-child(7) {
        transform: rotate(42deg) translateY($arc_radius);
    }
    .marks li:nth-child(8) {
        transform: rotate(48deg) translateY($arc_radius);
    }
    .marks li:nth-child(9) {
        transform: rotate(54deg) translateY($arc_radius);
    }
    .marks li:nth-child(10) {
        transform: rotate(60deg) translateY($arc_radius);
    }
    .marks li:nth-child(11) {
        transform: rotate(66deg) translateY($arc_radius);
    }
    .marks li:nth-child(12) {
        transform: rotate(72deg) translateY($arc_radius);
    }
    .marks li:nth-child(13) {
        transform: rotate(78deg) translateY($arc_radius);
    }
    .marks li:nth-child(14) {
        transform: rotate(84deg) translateY($arc_radius);
    }
    .marks li:nth-child(15) {
        transform: rotate(90deg) translateY($arc_radius);
    }
    .marks li:nth-child(16) {
        transform: rotate(96deg) translateY($arc_radius);
    }
    .marks li:nth-child(17) {
        transform: rotate(102deg) translateY($arc_radius);
    }
    .marks li:nth-child(18) {
        transform: rotate(108deg) translateY($arc_radius);
    }
    .marks li:nth-child(19) {
        transform: rotate(114deg) translateY($arc_radius);
    }
    .marks li:nth-child(20) {
        transform: rotate(120deg) translateY($arc_radius);
    }
    .marks li:nth-child(21) {
        transform: rotate(126deg) translateY($arc_radius);
    }
    .marks li:nth-child(22) {
        transform: rotate(132deg) translateY($arc_radius);
    }
    .marks li:nth-child(23) {
        transform: rotate(138deg) translateY($arc_radius);
    }
    .marks li:nth-child(24) {
        transform: rotate(144deg) translateY($arc_radius);
    }
    .marks li:nth-child(25) {
        transform: rotate(150deg) translateY($arc_radius);
    }
    .marks li:nth-child(26) {
        transform: rotate(156deg) translateY($arc_radius);
    }
    .marks li:nth-child(27) {
        transform: rotate(162deg) translateY($arc_radius);
    }
    .marks li:nth-child(28) {
        transform: rotate(168deg) translateY($arc_radius);
    }
    .marks li:nth-child(29) {
        transform: rotate(174deg) translateY($arc_radius);
    }
    .marks li:nth-child(30) {
        transform: rotate(180deg) translateY($arc_radius);
    }
    .marks li:nth-child(31) {
        transform: rotate(186deg) translateY($arc_radius);
    }
    .marks li:nth-child(32) {
        transform: rotate(192deg) translateY($arc_radius);
    }
    .marks li:nth-child(33) {
        transform: rotate(198deg) translateY($arc_radius);
    }
    .marks li:nth-child(34) {
        transform: rotate(204deg) translateY($arc_radius);
    }
    .marks li:nth-child(35) {
        transform: rotate(210deg) translateY($arc_radius);
    }
    .marks li:nth-child(36) {
        transform: rotate(216deg) translateY($arc_radius);
    }
    .marks li:nth-child(37) {
        transform: rotate(222deg) translateY($arc_radius);
    }
    .marks li:nth-child(38) {
        transform: rotate(228deg) translateY($arc_radius);
    }
    .marks li:nth-child(39) {
        transform: rotate(234deg) translateY($arc_radius);
    }
    .marks li:nth-child(40) {
        transform: rotate(240deg) translateY($arc_radius);
    }
    .marks li:nth-child(41) {
        transform: rotate(246deg) translateY($arc_radius);
    }
    .marks li:nth-child(42) {
        transform: rotate(252deg) translateY($arc_radius);
    }
    .marks li:nth-child(43) {
        transform: rotate(258deg) translateY($arc_radius);
    }
    .marks li:nth-child(44) {
        transform: rotate(264deg) translateY($arc_radius);
    }
    .marks li:nth-child(45) {
        transform: rotate(270deg) translateY($arc_radius);
    }
    .marks li:nth-child(46) {
        transform: rotate(276deg) translateY($arc_radius);
    }
    .marks li:nth-child(47) {
        transform: rotate(282deg) translateY($arc_radius);
    }
    .marks li:nth-child(48) {
        transform: rotate(288deg) translateY($arc_radius);
    }
    .marks li:nth-child(49) {
        transform: rotate(294deg) translateY($arc_radius);
    }
    .marks li:nth-child(50) {
        transform: rotate(300deg) translateY($arc_radius);
    }
    .marks li:nth-child(51) {
        transform: rotate(306deg) translateY($arc_radius);
    }
    .marks li:nth-child(52) {
        transform: rotate(312deg) translateY($arc_radius);
    }
    .marks li:nth-child(53) {
        transform: rotate(318deg) translateY($arc_radius);
    }
    .marks li:nth-child(54) {
        transform: rotate(324deg) translateY($arc_radius);
    }
    .marks li:nth-child(55) {
        transform: rotate(330deg) translateY($arc_radius);
    }
    .marks li:nth-child(56) {
        transform: rotate(336deg) translateY($arc_radius);
    }
    .marks li:nth-child(57) {
        transform: rotate(342deg) translateY($arc_radius);
    }
    .marks li:nth-child(58) {
        transform: rotate(348deg) translateY($arc_radius);
    }
    .marks li:nth-child(59) {
        transform: rotate(354deg) translateY($arc_radius);
    }
    .marks li:nth-child(60) {
        transform: rotate(360deg) translateY($arc_radius);
    }

    /*
    Some overrides.
    */
    .reactor-container.active {
    $arc_radius: 130px;
    $size3: 6px;
    $cshadow: rgba(2, 254, 255, 0.8);
    $marks_color_1: rgba(2, 254, 255, 1);
    $marks_color_2: rgba(2, 254, 255, 0.3);
    $colour1: rgba(2, 255, 255, 0.15);
    $colour3: rgba(2, 255, 255, 0.3);
    $cshadow: rgba(2, 254, 255, 0.8);

    scale: 1.1;
    opacity: 1;
    .coil-container {
        animation: 3s infinite linear reactor-anim;
    }

    .reactor-container-inner {
      box-shadow: 0px 0px 50px 15px $colour3, inset 0px 0px 50px 15px $colour3;
    }

    .core-inner {
        border: 5px solid #1b4e5f;
        background-color: #ffffff;
        box-shadow: 0px 0px 7px 5px #52fefe, 0px 0px 10px 10px #52fefe inset;
    }

  .core-outer {
    border: 1px solid #52fefe;
    background-color: #ffffff;
    box-shadow: 0px 0px 2px 1px #52fefe, 0px 0px 10px 5px #52fefe inset;
  }
  .core-wrapper {
    background-color: #073c4b;
    box-shadow: 0px 0px 5px 4px #52fefe, 0px 0px 6px 2px #52fefe inset;
  }
  .tunnel {
    background-color: #ffffff;
    box-shadow: 0px 0px 5px 1px #52fefe, 0px 0px 5px 4px #52fefe inset;
  }
  .coil {
    background-color: #073c4b;
    box-shadow: 0px 0px 5px #52fefe inset;
  }

  .semi_arc {
    border: 6px solid #02feff;
    background: rgba(2, 254, 255, 0.2);
  }

  
  .e5_1 {
        animation: rotate 3s linear infinite;
    }

    .e5_2 {
        animation: rotate_anti 2s linear infinite;
    }

    .e5_3 {
        animation: rotate 2s linear infinite;
    }

    .e5_4 {
        animation: rotate_anti 2s linear infinite;
    }
}
</style>